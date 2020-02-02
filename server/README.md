# Development

Running

```
RUST_BACKTRACE=1 RUST_LOG=INFO systemfd --no-pid -s http::8080 -- cargo watch -x run
```

# MVP

* Object types + state dict
* Save & load world + object state
* Run Lua code per type, exposing state, messaging, and world primitives
* Basic Lua room with a couple commands
* Deploy

Stretch 
* Lua CLI in UI
* Reload lua code from github via command