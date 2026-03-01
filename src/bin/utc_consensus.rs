use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, Copy)]
pub struct Estimation {
    pub d: f64, // Difference in seconds
    pub a: f64, // Uncertainty
}

pub fn estimate_offset(s: DateTime<Utc>, r: DateTime<Utc>, c: DateTime<Utc>) -> Estimation {
    // d = c - (r + s) / 2
    let send_receive_avg_ms = (r.timestamp_millis() + s.timestamp_millis()) / 2;
    let diff_ms = c.timestamp_millis() - send_receive_avg_ms;
    
    Estimation {
        d: diff_ms as f64 / 1000.0,
        a: (r.timestamp_millis() - s.timestamp_millis()) as f64 / 2000.0,
    }
}

pub struct SyncNode {
    pub id: usize,
    pub n: usize,
    pub f: usize,
    pub way_off: f64,
    pub adjustment: Duration,
    pub state: String,
}

impl SyncNode {
    pub fn new(id: usize, n: usize, f: usize, way_off: f64, initial_offset_sec: i64) -> Self {
        Self {
            id, n, f, way_off,
            adjustment: Duration::seconds(initial_offset_sec),
            state: String::from("Init"),
        }
    }

    pub fn get_logical_utc(&self) -> DateTime<Utc> {
        Utc::now() + self.adjustment
    }

    pub fn run_sync_cycle(&mut self, estimates: Vec<Estimation>) {
        let mut d_overs: Vec<f64> = estimates.iter().map(|e| e.d + e.a).collect();
        let mut d_unders: Vec<f64> = estimates.iter().map(|e| e.d - e.a).collect();

        d_overs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        d_unders.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let m = d_overs[self.f]; 
        let m_large = d_unders[self.n - 1 - self.f];

        let raw_adj_sec = if m > -self.way_off && m_large < self.way_off {
            self.state = String::from("Synced");
            (m.min(0.0) + m_large.max(0.0)) / 2.0 
        } else {
            self.state = String::from("RECOVERING");
            (m + m_large) / 2.0 
        };

        // Apply 40% of the correction (halving-style convergence)
        let apply_ms = (raw_adj_sec * 400.0) as i64;
        self.adjustment = self.adjustment + Duration::milliseconds(apply_ms);
    }
}

pub struct Network {
    pub nodes: Vec<SyncNode>,
    pub next_id: usize,
}

impl Network {
    pub fn new() -> Self {
        let mut nodes = Vec::new();
        for i in 0..10 {
            // Original nodes 0-3 are 1 hour (3600s) ahead
            let offset = if i < 4 { 3600 } else { 0 };
            nodes.push(SyncNode::new(i, 10, 3, 20.0, offset));
        }
        Network { nodes, next_id: 10 }
    }

    pub fn simulate_step(&mut self) -> f64 {
        let current_n = self.nodes.len();
        let mut all_estimates = Vec::new();

        for p_idx in 0..current_n {
            let mut p_estimates = Vec::new();
            let s = self.nodes[p_idx].get_logical_utc();
            for q_idx in 0..current_n {
                let c = self.nodes[q_idx].get_logical_utc();
                let r = self.nodes[p_idx].get_logical_utc();
                p_estimates.push(estimate_offset(s, r, c));
            }
            all_estimates.push(p_estimates);
        }

        let mut timestamps: Vec<i64> = Vec::new();
        for i in 0..current_n {
            let estimates = all_estimates[i].clone();
            self.nodes[i].run_sync_cycle(estimates);
            let time = self.nodes[i].get_logical_utc();
            timestamps.push(time.timestamp_millis());
            println!("Node {:03} | {:11} | UTC: {}", self.nodes[i].id, self.nodes[i].state, time.format("%H:%M:%S%.3f"));
        }
        
        let min_t = *timestamps.iter().min().unwrap();
        let max_t = *timestamps.iter().max().unwrap();
        (max_t - min_t) as f64 / 1000.0
    }
}

fn main() {
    let mut net = Network::new();
    let mut round = 1;
    let mut turnover_started = false;
    let mut turnover_complete = false;

    loop {
        println!("--- Round {:02} ---", round);
        let spread_sec = net.simulate_step();

        if spread_sec < 1.0 && !turnover_started {
            println!("\n>>> UTC SYNC REACHED. COMMENCING TOTAL TURNOVER. <<<\n");
            turnover_started = true;
        }

        if turnover_started && !turnover_complete {
            let to_drop = net.nodes.iter().find(|n| n.id < 10).map(|n| n.id);
            if let Some(id) = to_drop {
                println!(">>> DROPPING ORIGINAL Node {:03} <<<", id);
                net.nodes.retain(|n| n.id != id);
                
                let new_id = net.next_id;
                // Newcomer starts 2 hours (7200s) behind
                println!(">>> ADDING NEWCOMER Node {:03} (RECOVERING) <<<", new_id);
                net.nodes.push(SyncNode::new(new_id, 10, 3, 20.0, -7200));
                net.next_id += 1;
            } else {
                turnover_complete = true;
                println!("\n>>> TURNOVER COMPLETE. WAITING FOR NEWCOMER SYNC. <<<\n");
            }
        }

        let all_synced = net.nodes.iter().all(|n| n.state == "Synced");
        if turnover_complete && all_synced && spread_sec < 1.0 {
            println!("\n>>> SUCCESS: ALL NEWCOMERS SYNCED TO UTC CONSENSUS. <<<");
            break;
        }

        round += 1;
        if round > 500 { break; } 
    }
}
