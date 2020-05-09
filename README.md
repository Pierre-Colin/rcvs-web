# Web service for the Randomized Condorcet Voting System
This web server allows you to host an election using the Randomized Condorcet Voting System, an electoral system with very good game-theoretic properties. For mor details, see the repository for the [rcvs](https://github.com/Pierre-Colin/rcvs) crate.

## Architecture
All the back-end code is written in safe Rust. This server uses the [Actix](https://actix.rs/) HTTP library to provide both a [REST interface](https://en.wikipedia.org/wiki/Representational_state_transfer) and an HTML user interface. The application state has finely-grained shared locks so as to allow concurrent accesses, but is not wait-free. The crate [chashmap](https://crates.io/crates/chashmap) has not been used because it doesn’t allow iterating through `CHashMap`.

## To-do list
* The application state may store partial election results, but this is not fully implemented yet.
* The result HTML page.
* Switch to a better graph displaying library, such as graphviz.
* A manual HTML page.
* Authentication systems for both the admin and the electors that aren’t based on IP.
