use surrealdb::{Surreal, engine::remote::ws::Client};

use crate::models::users::UserModel;

pub struct UserService {
    pub db_conn: Surreal<Client>,
}

impl UserService {
    pub fn create_users(users: Vec<UserModel>) {}

    pub fn set_password(plain_password: String) -> String {
        "".to_string()
    }
}
