use num_bigint::BigUint;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    pub seed: String,
    pub diff: String,
    pub reward: f64,
    pub last_found: u64,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub seed: String,
    pub diff: BigUint,
    pub reward: f64,
    pub last_found: u64,
}

impl Job {
    pub fn get_wait_job() -> Job {
        // Returns empty job with seed ="wait" ment to make miner threads wait until job is set
        Job { seed: String::from("wait"), diff: BigUint::from(0_u32), reward: 0.0, last_found: 0 }
    }
    
    pub fn get_pause_job(&self) -> Job {
        // Returns empty job with seed ="wait" ment to make miner threads wait until job is set
        Job { seed: String::from("wait"), diff: self.diff.clone(), reward: self.reward.clone(), last_found: self.last_found.clone() }
    }
}

impl From<Body> for Job {
    fn from(body: Body) -> Self {
        Job {
            seed: body.seed,
            diff: BigUint::parse_bytes(body.diff.as_bytes(), 16).unwrap(),
            reward: body.reward,
            last_found: body.last_found,
        }
    }
}

pub async fn get_job(server: String) -> Result<Job, reqwest::Error> {
    let response = reqwest::get(format!("{}/get-challenge", server)).await?;
    let body: Body = response.json().await?;

    Ok(Job::from(body))
}
