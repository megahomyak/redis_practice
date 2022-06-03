use actix_web::{get, web, App, HttpResponse, HttpServer};
use log::error;
use num::BigUint;
use redis::{aio::Connection, AsyncCommands, RedisResult};
use serde::Deserialize;
use tokio::sync::Mutex;

type FactorialInputType = u32;

pub struct Seconds {
    pub seconds: usize,
}

pub struct State {
    pub redis_connection: Mutex<Connection>,
    pub upper_factorial_limit: FactorialInputType,
    pub default_cache_expiration_time: Seconds,
}

#[get("/")]
async fn index(
    query: Option<web::Query<FactorialProcessingQueryParams>>,
    state: web::Data<State>,
) -> HttpResponse {
    if let Some(query) = query {
        calculate_factorial(query.into_inner(), state).await
    } else {
        HttpResponse::Ok().body(include_str!("index.html"))
    }
}

#[derive(Deserialize)]
pub struct FactorialProcessingQueryParams {
    pub input_number: FactorialInputType,
}

async fn calculate_factorial<'output>(
    query_params: FactorialProcessingQueryParams,
    state: web::Data<State>,
) -> HttpResponse {
    let input_number = query_params.input_number;
    if input_number > state.upper_factorial_limit {
        return HttpResponse::Ok().body("The input number is too big!");
    }
    let cached_number: RedisResult<String> = state
        .redis_connection
        .lock()
        .await
        .get(input_number.to_string())
        .await;
    let (result, cache_status) = match cached_number {
        Ok(result) => (result, "hit"),
        Err(error) => {
            if error.kind() != redis::ErrorKind::TypeError {
                error!("{}", error);
            }
            let mut result = BigUint::new(vec![1]);
            for factor in 2..=input_number {
                result *= factor;
            }
            let result = result.to_string();
            if let Err(err) = state
                .redis_connection
                .lock()
                .await
                .set_ex::<String, String, ()>(
                    input_number.to_string(),
                    result.clone(),
                    state.default_cache_expiration_time.seconds,
                )
                .await
            {
                error!("{}", err);
            }
            (result, "miss")
        }
    };
    HttpResponse::Ok()
        .append_header(("X-Cache-Status", cache_status))
        .body(format!(include_str!("number.html"), result))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let redis_client = redis::Client::open("redis://127.0.0.1:6379/").unwrap();
    let state = web::Data::new(State {
        redis_connection: Mutex::new(redis_client.get_tokio_connection().await.unwrap()),
        upper_factorial_limit: 100_000,
        default_cache_expiration_time: Seconds {
            seconds: 10 * 60 * 60,
        },
    });
    HttpServer::new(move || App::new().app_data(state.clone()).service(index))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
