# Orisa

A quick hack building a Lua-based MOO in Rust with a React UI.

## Running Locally

* Clone `killpop` next to `orisa`. 

* Install Yarn.
* Install rustup.

* From client/ run `yarn start`.
* From server/ run `cargo run`.
  * or maybe install systemfd and use something like `RUST_BACKTRACE=1 RUST_LOG=INFO systemfd --no-pid -s http::8080 -- cargo watch -x run`

## Running on a server

* Clone `killpop` next to `orisa`. 
* `docker-compose up --build` will build & run two containers and expose on port 80.

Note that the config and dockerfiles are aimed at production use, not development.

# TODO

* presence
* simple commands like look, inspect
* commands for object/room/door creation and editing
* Reload lua code from github via command
* Make print! go to console logs, too
* ping/pong
* nicer eval allowing multiline, blocks, etc, like lua playground
* passwords
* capability model, isolated spaces
  * allow super calls which cross spaces, not just via code
