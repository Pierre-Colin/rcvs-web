use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
// use std::iter::FromIterator;
use std::collections::HashMap;
// use std::net::IpAddr;
use std::sync::{Arc, Mutex, RwLock};

use actix_files::NamedFile;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};

mod data;
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

#[derive(Serialize)]
struct NewResultData {
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
    open: bool,
    result: Option<Vec<ArrowData>>,
}

impl AppState {
    fn new(election_config: &str) -> Result<Self, Box<dyn Error>> {
        let file = File::open(election_config)?;
        let reader = BufReader::new(file);
        let election_data = serde_json::from_reader(reader)?;
        Ok(Self {
            election_data: election_data,
            database: Arc::new(Mutex::new(model::DatabaseConnection::new("model.db", "model.sql")?)),
            open: true,
            result: None,
        })
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
    
    if state.open {
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

    if state.open {
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

    let mut result_data = NewResultData {
        title: state.election_data.title.to_string(),
        alternatives: data.alternatives.to_vec(),
        arrows: Vec::new(),
        strategy: None,
        winner: None,
    };

    std::mem::drop(state_lock);

    let mut election = rcvs::Election::new();
    for alternative in data.alternatives {
        election.add_alternative(&(alternative.id as usize));
    }
    let mut ballots: HashMap<usize, rcvs::Ballot<usize>> = HashMap::new();
    for ranking in data.ballot {
        let elector = match ranking.elector {
            Some(elector) => elector,
            None => return HttpResponse::InternalServerError().body("Ranking in database has no elector"),
        };
        match ballots.get_mut(&elector) {
            Some(ballot) => if !(*ballot).insert(ranking.alternative, ranking.min, ranking.max) {
                return HttpResponse::InternalServerError().body("Ranking in database is invalid");
            },
            None => {
                let mut ballot = rcvs::Ballot::new();
                ballot.insert(ranking.alternative, ranking.min, ranking.max);
                ballots.insert(elector, ballot);
            },
        }
    }
    for (_, ballot) in ballots.iter() {
        election.cast(ballot.to_owned());
    }

    let graph = election.get_duel_graph();
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

    if let Ok(strategy) = graph.get_optimal_strategy() {
        result_data.strategy = Some(StrategyData::new(&strategy));
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

    state.open = false;

    HttpResponse::Ok().finish()
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

    state.open = true;

    HttpResponse::Ok().finish()
}

async fn vote_page() -> actix_web::Result<NamedFile> {
    let path: std::path::PathBuf = "vote.html".parse()?;
    let file = NamedFile::open(path)?
        .set_content_type("text/html; charset=utf-8".parse::<mime::Mime>().unwrap());
    Ok(file)
}

async fn result_page() -> actix_web::Result<NamedFile> {
    let path: std::path::PathBuf = "result.html".parse()?;
    let file = NamedFile::open(path)?
        .set_content_type("text/html; charset=utf-8".parse::<mime::Mime>().unwrap());
    Ok(file)
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
            .route("/vote", web::get().to(vote_page))
            .route("/result", web::get().to(result_page))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
