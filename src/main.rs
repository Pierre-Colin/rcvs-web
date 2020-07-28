use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
// use std::iter::FromIterator;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};

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

impl BallotData {
    fn check_errors(ballots: &[BallotData], alternatives: &[AlternativeData]) -> Result<(), BallotValidityError<String>> {
        for (i, ballot) in ballots.iter().enumerate() {
            if ballot.min > ballot.max {
                return Err(BallotValidityError::InvalidRankRange(ballot.min, ballot.max));
            }
            if !alternatives.iter().map(|alternative| &alternative.id).any(|id| id == &ballot.alternative) {
                return Err(BallotValidityError::AlternativeNotFound(ballot.alternative.to_string()));
            }
            for other in ballots.iter().take(i).rev() {
                if ballot.alternative == other.alternative {
                    return Err(BallotValidityError::DuplicateAlternative(ballot.alternative.to_string()));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct AppState {
    election_data: ElectionData,
    open: bool,
    ballots: Arc<RwLock<HashMap<IpAddr, Vec<BallotData>>>>,
    result: Option<ResultData<String>>,
}

impl AppState {
    fn new(election_config: &str) -> std::io::Result<Self> {
        let file = File::open(election_config)?;
        let reader = BufReader::new(file);
        let election_data = serde_json::from_reader(reader)?;
        Ok(Self {
            election_data: election_data,
            open: true,
            ballots: Arc::new(RwLock::new(HashMap::new())),
            result: None,
        })
    }

    fn compute_result<'a>(&'a mut self) -> Result<(), Box<dyn Error + 'a>> {
        let lock = match self.ballots.read() {
            Ok(l) => l,
            Err(what) => return Err(Box::new(what)),
        };

        let mut election = rcvs::Election::new();
        for alternative in &self.election_data.alternatives {
            election.add_alternative(&alternative.id);
        }

        for (_, ballot_data) in (*lock).iter() {
            let mut ballot = rcvs::Ballot::new();
            for entry in ballot_data {
                ballot.insert(entry.alternative.to_string(), entry.min, entry.max);
            }
            election.cast(ballot);
        }

        std::mem::drop(lock);

        self.result = ResultData::from_election(&self.election_data.title, &election);
        Ok(())
    }
}

type SharedState = web::Data<Arc<RwLock<AppState>>>;

async fn get_info(req: HttpRequest, state: SharedState) -> impl Responder {
    let ip = match req.peer_addr() {
        Some(a) => a.ip(),
        None => return HttpResponse::InternalServerError()
            .body("Failed to retrieve client IP address"),
    };

    let mut data = match model::get_data(&ip.to_string()) {
        Ok(data) => data,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Failed to query data base: {}", what)),
    };

    let state_lock = match state.read() {
        Ok(l) => l,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Mutex poisoned: {}", what)),
    };
    let state = &*state_lock;

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

async fn post_ballot(req: HttpRequest, ballot: web::Json<Vec<model::BallotRow>>) -> impl Responder {
    let ip = match req.peer_addr() {
        Some(a) => a.ip(),
        None => return HttpResponse::InternalServerError()
            .body("Failed to retrieve client IP address"),
    };

    if let Err(what) = check_ballot_shape(&ballot) {
        return HttpResponse::BadRequest().body(&format!("Bad ballot format: {}", what));
    }

    match model::set_ballot(&ip.to_string(), &ballot) {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(what) => HttpResponse::InternalServerError()
            .body(&format!("Failed to post ballot: {}", what)),
    }
}

async fn delete_ballot(req: HttpRequest) -> impl Responder {
    let ip = match req.peer_addr() {
        Some(a) => a.ip(),
        None => return HttpResponse::InternalServerError()
            .body("Failed to retrieve client IP address"),
    };

    match model::delete_ballot(&ip.to_string()) {
        Ok(true) => HttpResponse::NoContent().finish(),
        Ok(false) => HttpResponse::NotFound().body("No ballot detected"),
        Err(what) => HttpResponse::InternalServerError()
            .body(&format!("Failed to delete ballot: {}", what)),
    }
}

async fn result() -> impl Responder {
    let data = match model::collect_votes() {
        Ok(data) => data,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Failed to collect ballots: {}", what)),
    };

    let mut election = rcvs::Election::new();
    for alternative in data.alternatives {
        election.add_alternative(&(alternative.id as usize));
    }
    for ranking in data.ballot {
        let mut ballot = rcvs::Ballot::new();
        ballot.insert(ranking.alternative, ranking.min, ranking.max);
        election.cast(ballot);
    }

    HttpResponse::Ok().json(ResultData::from_election("Foo", &election))
}

async fn result_old(state: SharedState) -> impl Responder {
    let state_lock = match state.read() {
        Ok(l) => l,
        Err(what) => return HttpResponse::InternalServerError()
            .body(&format!("Mutex poisoned: {}", what)),
    };
    let state = &*state_lock;

    let mut election = rcvs::Election::new();
    for alternative in &state.election_data.alternatives {
        election.add_alternative(&alternative.id);
    }

    if state.open {
        let lock = match state.ballots.read() {
            Ok(l) => l,
            Err(what) => return HttpResponse::InternalServerError()
                .body(&format!("Mutex poisoned: {}", what)),
        };

        for (_, ballot_data) in (*lock).iter() {
            let mut ballot = rcvs::Ballot::new();
            for ballot_entry in ballot_data {
                ballot.insert(ballot_entry.alternative.to_string(), ballot_entry.min, ballot_entry.max);
            }
            election.cast(ballot);
        }

        std::mem::drop(lock);

        return HttpResponse::Ok().json(ResultData::from_election(&(*state_lock).election_data.title, &election));
    } else {
        let lock = match state.ballots.read() {
            Ok(l) => l,
            Err(what) => return HttpResponse::InternalServerError()
                .body(&format!("Mutex poisoned: {}", what)),
        };

        for (_, ballot_data) in (*lock).iter() {
            let mut ballot = rcvs::Ballot::new();
            for ballot_entry in ballot_data {
                ballot.insert(ballot_entry.alternative.to_string(), ballot_entry.min, ballot_entry.max);
            }
            election.cast(ballot);
        }

        std::mem::drop(lock);

        return HttpResponse::Ok().json(ResultData::from_election(&(*state_lock).election_data.title, &election));
    }
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

    match state.compute_result() {
        Ok(()) => (),
        Err(what) => return HttpResponse::InternalServerError().body(&format!("{}", what)),
    }
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
    let app_state = Arc::new(RwLock::new(AppState::new("election.json")?));
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
