### setup
```cargo b -r```
### use
```usage: ./target/release/coreping <main_core> <worker_core> <timeout_seconds>```
### example
```perf stat -d -r 5 ./target/release/coreping 1 0 10```
