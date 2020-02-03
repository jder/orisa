# Development

Running

```
RUST_BACKTRACE=1 RUST_LOG=INFO systemfd --no-pid -s http::8080 -- cargo watch -x run
```

# MVP
* Deploy

# TODO

* Send lua code from UI to be evalued in your user object
* Reload lua code from github via command
* private object state (orisa.set_state("foo", "whatever")) and public attrs orisa.get_attr(object, "foo") / orisa.set_attr("foo", "something")
* capability model