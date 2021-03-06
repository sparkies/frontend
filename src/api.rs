use rocket::Outcome;
use rocket::http::{Cookie, Cookies};
use rocket::request::{self, Request, FromRequest};
use rocket_contrib::{Json, JsonValue};

use bcrypt;

use db::{self, DbConn};
use db::models::*;
use db::schema::users::dsl::*;
use diesel;
use diesel::prelude::*;

use super::info::InfoSet;

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    content: String,
    dest: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct Login {
    user: String,
    pass: String,
}

/// Represents a user who is authorized via private cookies.
/// A user will become authorized once they login with
/// the proper credentials using the /api/login endpoint.
pub struct AuthedUser;

/// Controls how an authorized user's requests are handled.
/// If a user is authenticated, it will succeed. Otherwise
/// the request will be forwarded to another handler.
impl<'a, 'r> FromRequest<'a, 'r> for AuthedUser {
    type Error = ();

    fn from_request(request: &'a Request<'r>) -> request::Outcome<AuthedUser, ()> {
        match request.cookies().get_private("auth") {
            Some(_) => Outcome::Success(AuthedUser),
            None => Outcome::Forward(()),
        }
    }
}

/// Sends the data given to the xbee network.
/// 
/// This endpoint takes JSON data that contains both the
/// destination node's id and the content of the message.
/// 
/// **Note**: This endpoint requires that the user is authorized.
/// 
/// # Example
/// ```json
/// {
///     "content": "Data to send",
///     "dest": 1234
/// }
/// ```
#[post("/api/send", format = "application/json", data = "<message>")]
fn send(message: Json<Message>, _user: AuthedUser) -> JsonValue {
    info!("JSON: {:?}", message);
    json!({
        "content": message.content.clone(),
        "success": true,
    })
}

/// A temporary endpoint that adds the given data to the database.
/// 
/// This endpoint takes JSON data that describes an Xbee. 
/// 
/// **Note**: This endpoint requires that the user is authorized.
/// 
/// # Example
/// ```json
/// {
///     "node_id": 1234,
///     "name": "Temperature Sensor",
///     "units": "C"
/// }
/// ```
#[post("/api/add", format = "application/json", data = "<xbee>")]
fn add(xbee: Json<NewXbee>, conn: DbConn, _user: AuthedUser) -> JsonValue {
    db::create_xbee(&conn, xbee.node_id, &xbee.name, &xbee.units);

    json!({
        "success": true,
    })
}

/// Returns a list of active nodes and their most recent values.
/// This data will be returned as a JSON object where the xbee
/// data is stored in an array.
/// 
/// **Note**: This endpoint requires that the user is authorized.
/// 
/// # Example
/// ```json
/// {
///     "nodes": [{
///         "last_update": 1523568385,
///         "max_value": 150.0,
///         "max_voltage": 5.0,
///         "min_value": 0.0,
///         "min_voltage": 0.0,
///         "name": "Test",
///         "reading": 413,
///         "units": "C",
///         "uuid": 2
///     }, {
///         ...
///     }],
///     "success": true
/// }
/// ```
#[get("/api/list")]
fn list_authed(info: InfoSet, _user: AuthedUser) -> JsonValue {
    json!({
        "nodes": info.nodes(),
        "success": true,
    })
}

/// This is an error handler for the /api/list endpoint
/// that is called when the user is not authorized. No
/// xbee data will be returned from this endpoint, just
/// a simple JSON object that indicates failure.
#[get("/api/list", rank = 2)]
fn list_invalid() -> JsonValue {
    json!({
        "success": false,
    })
}

/// This is a login endpoint for users to authenticate themselves.
/// A username and password must be supplied in a JSON object as
/// described below. Once a user is authenticated, a private cookie
/// will be stored which will allow them to access endpoints that
/// require authentication.
/// 
/// # Errors
/// If the given username is not in the database, an error noting
/// that will be returned.
/// 
/// If a valid username is given but the password is wrong, an error
/// will be returned.
/// 
/// If any other database error occurs it will return a generic error.
#[post("/api/login", format = "application/json", data = "<login>")]
fn login(login: Json<Login>, conn: DbConn, mut cookies: Cookies) -> JsonValue {
    //  Try to find a user in the database with the given username.
    //  This query returns at most 1 result.
    let res = users
        .filter(username.eq(&login.user))
        .get_result::<User>(&*conn);

    match res {
        //  User was found, so now check the password.
        Ok(user) => {
            //  Password is stored as a bcrypt hash so we need to
            //  ensure it is correct.
            if let Ok(true) = bcrypt::verify(&login.pass, &user.password) {
                //  Password matched hash, add authenticated cookie.
                cookies.add_private(Cookie::new("auth", "true"));

                json!({
                    "success": true,
                })
            } else {
                //  Either the hash check failed, or the hash didn't match.
                //  Either way, return invalid credentials.
                json!({
                    "error": "Invalid login credentials.",
                    "success": false,
                })
            }
        }
        //  User was not found in the database.
        Err(diesel::result::Error::NotFound) => {
            json!({
                "error": "No user with that name found.",
                "success": false,
            })
        }
        //  Another database error occurred.
        Err(_) => {
            json!({
                "error": "Error getting information from database.",
                "success": false,
            })
        }
    }
}

/// This endpoint removes the authentication cookie. Once
/// called, a user can no longer access authenticated endpoints.
#[get("/api/logout")]
fn logout(mut cookies: Cookies) -> JsonValue {
    cookies.remove_private(Cookie::new("auth", "true"));
    json!({
        "success": true,
    })
}