-- User table definition

DEFINE TABLE users;

DEFINE FIELD name ON TABLE users TYPE string;
DEFINE FIELD email ON TABLE users TYPE string;
DEFINE FIELD hashed_password ON TABLE users TYPE string;

-- User group table definition
DEFINE TABLE groups;

-- User permissions table definition
DEFINE TABLE user_permissions;
