# Web service for the Randomized Condorcet Voting System
This web server allows you to host an election using the Randomized Condorcet Voting System, an electoral system with very good game-theoretic properties. For mor details, see the repository for the [rcvs](https://github.com/Pierre-Colin/rcvs) crate.

## Architecture
All the back-end code is written in safe Rust. This server uses the [Actix](https://actix.rs/) HTTP library to provide both a [REST interface](https://en.wikipedia.org/wiki/Representational_state_transfer) and an HTML user interface. The application state has finely-grained shared locks so as to allow concurrent accesses, but is not wait-free. The crux of the application data is stored in an [SQLite](https://www.sqlite.org/index.html) database. Since SQLite is protected _via_ mutual exclusion (both in this application and [internally in SQLite](https://www.sqlite.org/faq.html#q6)), the system is not lock-free. It is nevertheless aimed to be sequentially consistent (though proving it actually is must wait until the opening/closing feature is complete).

## To-do list
This list is ordered in order of perceived priority.
* The application state may store partial election results, but this is not implemented yet.
* First-come-first-served verification that the election is open/closed.
* A manual HTML page introducing the voting system itself.
* Modify the election after it started.
* Switch to a better graph displaying library, such as graphviz.
* Authentication systems for both the admin and the electors that aren’t based on IP addresses.
* Switch to a DBMS with better concurrency support, maybe with [Diesel](http://diesel.rs/).
