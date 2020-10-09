use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::mem;
use std::sync::{Arc, Mutex, RwLock};

use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};

mod data;
mod html_interface;
mod model;

use data::*;

#[derive(Deserialize, Clone, Debug)]
struct ElectionData {
    title: String,
    alternatives: Vec<AlternativeData>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct BallotData {
    alternative: String,
    min: u64,
    max: u64,
}

#[derive(Clone, Debug)]
enum BallotValidityError<V> {
    // May be used to intercept SQL error
    #[allow(dead_code)]
    AlternativeNotFound(V),
    InvalidRankRange(u64, u64),
    DuplicateAlternative(V),
}

impl<V: fmt::Debug + fmt::Display> Error for BallotValidityError<V> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl<V: fmt::Display> fmt::Display for BallotValidityError<V> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::AlternativeNotFound(a) => write!(f, "{} is not a valid alternative", a),
            Self::InvalidRankRange(a, b) => write!(f, "[{}, {}] is not a valid range", a, b),
            Self::DuplicateAlternative(a) => write!(f, "{} appears twice in the ballot", a),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
struct ArrowData {
    from: usize,
    to: usize,
}

#[derive(Clone, Debug, Serialize)]
struct ResultData {
    title: String,
    alternatives: Vec<model::AlternativeData>,
    arrows: Vec<ArrowData>,
    strategy: Option<StrategyData<usize>>,
    winner: Option<usize>,
}

#[derive(Clone, Debug)]
struct AppState {
    election_data: ElectionData,
    database: Arc<Mutex<model::DatabaseConnection>>,
    result: Option<ResultData>,
}

impl AppState {
    fn new(election_config: &str) -> Result<Self, Box<dyn Error>> {
        let file = File::open(election_config)?;
        let reader = BufReader::new(file);
        let election_data: ElectionData = serde_json::from_reader(reader)?;
        let connection = model::DatabaseConnection::new("model.db", "model.sql", &election_data.alternatives)?;
        Ok(Self {
            election_data: election_data,
            database: Arc::new(Mutex::new(connection)),
            result: None,
        })
    }

    fn is_open(&self) -> bool {
        self.result.is_none()
    }
}

type SharedState = web::Data<Arc<RwLock<AppState>>>;

async fn get_info(req: HttpRequest, state: SharedState) -> impl Responder {
    let ip = match req.peer_addr() {
        Some(a) => a.ip(),
        None => return HttpResponse::InternalServerError()
            .body("Failed to retrieve client IP address"),
    };
    
    let state_lock = match state.read() {
        Ok(l) => l,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Mutex poisoned: {}", what)),
    };
    let state = &*state_lock;

    let mut database_lock = match state.database.lock() {
        Ok(l) => l,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Mutex poisoned: {}", what)),
    };

    let mut data = match model::get_data(&mut *database_lock, &ip.to_string()) {
        Ok(data) => data,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Failed to query data base: {}", what)),
    };

    std::mem::drop(database_lock);

    data.title = Some(state.election_data.title.to_string());

    HttpResponse::Ok().json(data)
}

fn check_ballot_shape(ballot: &[model::BallotRow]) -> Result<(), BallotValidityError<usize>> {
    let mut found = vec![false; ballot.len()];
    for row in ballot {
        match found.get(row.alternative - 1) {
            Some(true) => return Err(
                BallotValidityError::DuplicateAlternative(row.alternative)
            ),
            Some(false) => found[row.alternative - 1] = true,
            None => {
                found.resize(row.alternative, false);
                found[row.alternative - 1] = true;
            },
        }
        if row.min > row.max {
            return Err(BallotValidityError::InvalidRankRange(row.min, row.max));
        }
    }
    Ok(())
}

async fn post_ballot(req: HttpRequest, ballot: web::Json<Vec<model::BallotRow>>, state: SharedState) -> impl Responder {
    let ip = match req.peer_addr() {
        Some(a) => a.ip(),
        None => return HttpResponse::InternalServerError()
            .body("Failed to retrieve client IP address"),
    };

    if let Err(what) = check_ballot_shape(&ballot) {
        return HttpResponse::BadRequest().body(&format!("Bad ballot format: {}", what));
    }

    let state_lock = match state.read() {
        Ok(l) => l,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Mutex poisoned: {}", what)),
    };
    let state = &*state_lock;
    
    if state.is_open() {
        let mut database_lock = match state.database.lock() {
            Ok(l) => l,
            Err(what) => return HttpResponse::InternalServerError()
                .body(&format!("Mutex poisoned: {}", what)),
        };

        match model::set_ballot(&mut *database_lock, &ip.to_string(), &ballot) {
            Ok(()) => HttpResponse::NoContent().finish(),
            Err(what) => HttpResponse::InternalServerError()
                .body(&format!("Failed to post ballot: {}", what)),
        }
    } else {
        HttpResponse::Forbidden().body("Election is closed")
    }
}

async fn delete_ballot(req: HttpRequest, state: SharedState) -> impl Responder {
    let ip = match req.peer_addr() {
        Some(a) => a.ip(),
        None => return HttpResponse::InternalServerError()
            .body("Failed to retrieve client IP address"),
    };

    let state_lock = match state.read() {
        Ok(l) => l,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Mutex poisoned: {}", what)),
    };
    let state = &*state_lock;

    if state.is_open() {
        let database_lock = match state.database.lock() {
            Ok(l) => l,
            Err(what) => return HttpResponse::InternalServerError()
                .body(&format!("Mutex poisoned: {}", what)),
        };

        match model::delete_ballot(&*database_lock, &ip.to_string()) {
            Ok(true) => HttpResponse::NoContent().finish(),
            Ok(false) => HttpResponse::NotFound().body("No ballot detected"),
            Err(what) => HttpResponse::InternalServerError()
                .body(&format!("Failed to delete ballot: {}", what)),
        }
    } else {
        HttpResponse::Forbidden().body("Election is closed")
    }
}

async fn result(state: SharedState) -> impl Responder {
    let state_lock = match state.read() {
        Ok(lock) => lock,
        Err(what) => return HttpResponse::InternalServerError().body(&format!("Mutex poisoned: {}", what)),
    };
    let state = &*state_lock;

    if let Some(result) = &state.result {
        return HttpResponse::Ok().json(result);
    }

    let mut database_lock = match state.database.lock() {
        Ok(lock) => lock,
        Err(what) => return HttpResponse::InternalServerError().body(&format!("Mutex poisoned: {}", what)),
    };

    let data = match model::collect_votes(&mut *database_lock) {
        Ok(data) => data,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Failed to collect ballots: {}", what)),
    };

    std::mem::drop(database_lock);

    let mut result_data = ResultData {
        title: state.election_data.title.to_string(),
        alternatives: data.alternatives.to_vec(),
        arrows: Vec::new(),
        strategy: None,
        winner: None,
    };

    std::mem::drop(state_lock);

    let graph = rcvs::build_graph(data.alternatives.iter().map(|x| x.id as usize), data.ballots.iter().map(|(_, x)| x.to_owned()));
    for (i, alternative) in graph.get_vertices().iter().enumerate() {
        for (j, other) in graph.get_vertices().iter().enumerate() {
            if i != j && graph[(i, j)] {
                result_data.arrows.push(ArrowData {
                    from: *alternative,
                    to: *other,
                });
            }
        }
    }

    match graph.get_optimal_strategy() {
        Ok(strategy) => result_data.strategy = Some(StrategyData::new(&strategy)),
        Err(what) => eprintln!("Error: {}", what),
    }

    HttpResponse::Ok().json(result_data)
}

async fn close(req: HttpRequest, state: SharedState) -> impl Responder {
    let ip = match req.peer_addr() {
        Some(a) => a.ip(),
        None => return HttpResponse::InternalServerError()
            .body("Failed to retrieve client IP address"),
    };

    if !ip.is_loopback() {
        return HttpResponse::Forbidden().body("Only loopback can close the election");
    }

    let mut state_lock = match state.write() {
        Ok(l) => l,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Mutex poisoned: {}", what)),
    };
    let state = &mut *state_lock;

    let mut database_lock = match state.database.lock() {
        Ok(lock) => lock,
        Err(what) => return HttpResponse::InternalServerError().body(&format!("Mutex poisoned: {}", what)),
    };

    let data = match model::collect_votes(&mut *database_lock) {
        Ok(data) => data,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Failed to collect ballots: {}", what)),
    };

    std::mem::drop(database_lock);

    let mut result_data = ResultData {
        title: state.election_data.title.to_string(),
        alternatives: data.alternatives.to_vec(),
        arrows: Vec::new(),
        strategy: None,
        winner: None,
    };
    
    let graph = rcvs::build_graph(data.alternatives.iter().map(|x| x.id as usize), data.ballots.iter().map(|(_, x)| x.to_owned()));
    for (i, alternative) in graph.get_vertices().iter().enumerate() {
        for (j, other) in graph.get_vertices().iter().enumerate() {
            if i != j && graph[(i, j)] {
                result_data.arrows.push(ArrowData {
                    from: *alternative,
                    to: *other,
                });
            }
        }
    }
    
    match graph.get_optimal_strategy() {
        Ok(strategy) => {
            result_data.strategy = Some(StrategyData::new(&strategy));
            result_data.winner = strategy.play(&mut rand::thread_rng());
        },
        Err(what) => eprintln!("Error: {}", what),
    }

    state.result = Some(result_data);
    mem::drop(state_lock);
    println!("Election has been closed");

    HttpResponse::NoContent().finish()
}

async fn open(req: HttpRequest, state: SharedState) -> impl Responder {
    let ip = match req.peer_addr() {
        Some(a) => a.ip(),
        None => return HttpResponse::InternalServerError()
            .body("Failed to retrieve client IP address"),
    };

    if !ip.is_loopback() {
        return HttpResponse::Forbidden().body("Only loopback can close the election");
    }

    let mut state_lock = match state.write() {
        Ok(l) => l,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Mutex poisoned: {}", what)),
    };
    let state = &mut *state_lock;

    state.result = None;
    mem::drop(state_lock);
    println!("Election has been open");

    HttpResponse::NoContent().finish()
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let app_state = Arc::new(RwLock::new(AppState::new("election.json").expect("Failed to initialize application state")));
    HttpServer::new(move || {
        App::new()
            .data(app_state.clone())
            .service(
                web::scope("/api")
                .route("/", web::get().to(get_info))
                .route("/ballot", web::get().to(get_info))
                .route("/ballot", web::post().to(post_ballot))
                .route("/ballot", web::delete().to(delete_ballot))
                .route("/result", web::get().to(result))
                .route("/close", web::get().to(close))
                .route("/open", web::get().to(open))
            )
            .route("/vote", web::get().to(html_interface::vote))
            .route("/result", web::get().to(html_interface::result))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
