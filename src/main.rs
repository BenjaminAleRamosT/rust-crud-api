use postgres::{Client, NoTls};
use postgres::Error as PostgresError;
use postgres::error::SqlState;
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::sync::LazyLock;

#[macro_use]
extern crate serde_derive;

//Model: User struct with id, name, email
#[derive(Serialize, Deserialize)]
struct User {
    id: Option<i32>,
    name: String,
    email: String
}

//DATABASE_URL
static DB_URL: LazyLock<String> =
    LazyLock::new(|| std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"));

const OK_RESPONSE: &str = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n";
const NOT_FOUND : &str = "HTTP/1.1 404 NOT FOUND\r\n\r\n";
const CONFLICT : &str = "HTTP/1.1 409 CONFLICT\r\n\r\n";
const INTERNAL_SERVER_ERROR : &str = "HTTP/1.1 500 INTERNAL SERVER ERROR\r\n\r\n";

// main function 
fn main() {
    //set database 
    if let Err(e) = set_database() {
        eprintln!("Error setting up database: {}", e);
        return;
    }

    //start server
    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();
    println!("Server listening on port 8080");

    //handle the client
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                handle_client(stream);
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }


}

//handle client function
fn handle_client(mut stream: TcpStream) {
    let mut buffer = [0; 1024];
    let mut request = String::new();

    match stream.read(&mut buffer){
        Ok(size) => {
            request.push_str(&String::from_utf8_lossy(&buffer[..size]).as_ref());
            let (status_line, content) = match &*request {
                r if r.starts_with("POST /users") => handle_post_request(r),
                r if r.starts_with("GET /users/") => handle_get_request(r),
                r if r.starts_with("GET /users") => handle_get_all_request(r),
                r if r.starts_with("PUT /users/") => handle_put_request(r),
                r if r.starts_with("DELETE /users/") => handle_delete_request(r),
                _ => (NOT_FOUND.to_string(), "Not Found".to_string()),
            };

            stream.write_all(format!("{}{}", status_line, content).as_bytes()).unwrap();

        }
        Err(e) => {
            eprintln!("Error reading from stream: {}", e);
            
        }
    }

}

//CONTROLLERS
//handle_post_request function
fn handle_post_request(request: &str) -> (String, String) {
    match (get_user_request_body(&request), Client::connect(DB_URL.as_str(), NoTls)) {
        (Ok(user), Ok(mut client)) => {

            match client.execute(
                "INSERT INTO users (name, email) VALUES ($1, $2)",
                &[&user.name, &user.email]
            ) {
                Ok(_) => (OK_RESPONSE.to_string(), "User created".to_string()),
                Err(e) => db_error_response(&e),
            }
        }
        _ => 
            (INTERNAL_SERVER_ERROR.to_string(), "Internal Server Error".to_string())
        
    }
}

fn handle_get_request(request: &str) -> (String, String) {
    match (get_id(&request).parse::<i32>(), Client::connect(DB_URL.as_str(), NoTls)) {
        
        (Ok(id), Ok(mut client)) =>
            match client.query_one("SELECT id, name, email FROM users WHERE id = $1 AND status = 1", &[&id]) {
            Ok(row) => {
                let user = User {
                    id: row.get(0),
                    name: row.get(1),
                    email: row.get(2),
                };
                
                (OK_RESPONSE.to_string(), serde_json::to_string(&user).unwrap())
                
            }
            _ => (NOT_FOUND.to_string(), "User not found".to_string()),
        }
        _ => (INTERNAL_SERVER_ERROR.to_string(), "Internal Server Error".to_string()),
    }
}


//handle_get_all_request function
fn handle_get_all_request(request: &str) -> (String, String) {
    match Client::connect(DB_URL.as_str(), NoTls) {
        Ok(mut client) => {
            let mut users = Vec::new();
            for row in client.query("SELECT id, name, email FROM users WHERE status = 1", &[]).unwrap() {
                users.push(User {
                    id: row.get(0),
                    name: row.get(1),
                    email: row.get(2),
                });
            }

            (OK_RESPONSE.to_string(), serde_json::to_string(&users).unwrap())
        }
        _ => (INTERNAL_SERVER_ERROR.to_string(), "Internal Server Error".to_string()),
    }
}

//handle_put_request function
fn handle_put_request(request: &str) -> (String, String) {
    match (
        get_id(&request).parse::<i32>(),
         get_user_request_body(&request),
            Client::connect(DB_URL.as_str(), NoTls)) {
        (Ok(id), Ok(user), Ok(mut client)) => {
            match client.execute(
                "UPDATE users SET name = $1, email = $2 WHERE id = $3 AND status = 1",
                &[&user.name, &user.email, &id]
            ) {
                Ok(0) => (NOT_FOUND.to_string(), "User not found".to_string()),
                Ok(_) => (OK_RESPONSE.to_string(), "User updated".to_string()),
                Err(e) => db_error_response(&e),
            }
        }
        _ => (INTERNAL_SERVER_ERROR.to_string(), "Internal Server Error".to_string()),
    }
}

//handle_delete_request function
fn handle_delete_request(request: &str) -> (String, String) {
    match (get_id(&request).parse::<i32>(), Client::connect(DB_URL.as_str(), NoTls)) {
            (Ok(id), Ok(mut client)) => {
                let rows_affected = client.execute(
                    "UPDATE users SET status = 0 WHERE id = $1 AND status = 1",
                    &[&id]
                ).unwrap();
                
                if rows_affected == 0 {
                    return (NOT_FOUND.to_string(), "User not found".to_string());
                }
    
                (OK_RESPONSE.to_string(), "User deleted".to_string())
            }
            _ => (INTERNAL_SERVER_ERROR.to_string(), "Internal Server Error".to_string()),
        }
    }


    

  //set database function
fn set_database() -> Result<(), PostgresError> {
    let mut client = Client::connect(DB_URL.as_str(), NoTls)?;

    //create users table if not exists
    client.batch_execute("
        CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            name VARCHAR NOT NULL,
            email VARCHAR NOT NULL UNIQUE,
            status INTEGER NOT NULL DEFAULT 1
        )
    ")?;

   
    Ok(())
}


//maps a database error to an HTTP response (409 if the email already exists)
fn db_error_response(e: &PostgresError) -> (String, String) {
    if e.code() == Some(&SqlState::UNIQUE_VIOLATION) {
        (CONFLICT.to_string(), "Email already exists".to_string())
    } else {
        eprintln!("Database error: {}", e);
        (INTERNAL_SERVER_ERROR.to_string(), "Internal Server Error".to_string())
    }
}

//get_id function
fn get_id(request: &str) -> &str{
    request.split("/").nth(2).unwrap_or_default().split_whitespace().next().unwrap_or_default()
}


//desealize user from request body with the id 
fn get_user_request_body(request: &str) -> Result<User, serde_json::Error> {
    let body: &str = request.split("\r\n\r\n").last().unwrap_or_default();
    serde_json::from_str(body)
}