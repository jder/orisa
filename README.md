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

* sessions so websocket reconnect re-connects to user
* chat history
* passwords
* ping/pong
* make /login need to be at start of a line
* private object state (orisa.set_state("foo", "whatever")) and public attrs orisa.get_attr(object, "foo") / orisa.set_attr("foo", "something")
* presence
* Send lua code from UI to be evalued in your user object
* capability model
* Reload lua code from github via command
