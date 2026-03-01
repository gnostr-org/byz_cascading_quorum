use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct Estimation {
    pub d: f64, 
    pub a: f64, 
}

pub fn estimate_offset(s: f64, r: f64, c: f64) -> Estimation {
    Estimation {
        d: c - (r + s) / 2.0,
        a: (r - s) / 2.0,
    }
}

pub struct SyncNode {
    pub id: usize,
    pub n: usize,
    pub f: usize,
    pub way_off: f64,
    pub adj_p: f64,
    pub state: String,
}

impl SyncNode {
    pub fn new(id: usize, n: usize, f: usize, way_off: f64, initial_adj: f64) -> Self {
        Self {
            id, n, f, way_off,
            adj_p: initial_adj,
            state: String::from("Init"),
        }
    }

    pub fn get_local_time(&self) -> f64 {
        self.adj_p 
    }

    pub fn run_sync_cycle(&mut self, estimates: Vec<Estimation>) {
        let mut d_overs: Vec<f64> = estimates.iter().map(|e| e.d + e.a).collect();
        let mut d_unders: Vec<f64> = estimates.iter().map(|e| e.d - e.a).collect();

        d_overs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        d_unders.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let m = d_overs[self.f]; 
        let m_large = d_unders[self.n - 1 - self.f];

        let raw_adjustment = if m > -self.way_off && m_large < self.way_off {
            self.state = String::from("Synced");
            (m.min(0.0) + m_large.max(0.0)) / 2.0 
        } else {
            self.state = String::from("RECOVERING");
            (m + m_large) / 2.0 
        };

        // 40% correction to visualize the halving convergence property
        self.adj_p += raw_adjustment * 0.4; 
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
            let offset = if i < 4 { 1000.0 } else { 0.0 };
            nodes.push(SyncNode::new(i, 10, 3, 20.0, offset));
        }
        Network { nodes, next_id: 10 }
    }

    pub fn simulate_step(&mut self) -> f64 {
        let current_n = self.nodes.len();
        let mut all_estimates = Vec::new();

        for p_idx in 0..current_n {
            let mut p_estimates = Vec::new();
            let s = self.nodes[p_idx].get_local_time();
            for q_idx in 0..current_n {
                let c = self.nodes[q_idx].get_local_time();
                let r = self.nodes[p_idx].get_local_time();
                p_estimates.push(estimate_offset(s, r, c));
            }
            all_estimates.push(p_estimates);
        }

        let mut min_t = f64::MAX;
        let mut max_t = f64::MIN;

        for i in 0..current_n {
            let estimates = all_estimates[i].clone();
            self.nodes[i].run_sync_cycle(estimates);
            let time = self.nodes[i].get_local_time();
            if time < min_t { min_t = time; }
            if time > max_t { max_t = time; }
            println!("Node {:03} | {:11} | Clock: {:7.2}", self.nodes[i].id, self.nodes[i].state, time);
        }
        max_t - min_t
    }
}

fn main() {
    let mut net = Network::new();
    let mut round = 1;
    let mut turnover_started = false;
    let mut turnover_complete = false;

    loop {
        println!("--- Round {:02} ---", round);
        let spread = net.simulate_step();

        // Phase 1 -> Phase 2 Transition
        if spread < 1.0 && !turnover_started {
            println!("\n>>> SYNC REACHED. COMMENCING TOTAL TURNOVER. <<<\n");
            turnover_started = true;
        }

        // Phase 2: Dropping originals
        if turnover_started && !turnover_complete {
            let to_drop = net.nodes.iter().find(|n| n.id < 10).map(|n| n.id);
            if let Some(id) = to_drop {
                println!(">>> DROPPING ORIGINAL Node {:03} <<<", id);
                net.nodes.retain(|n| n.id != id);
                
                let new_id = net.next_id;
                println!(">>> ADDING NEWCOMER Node {:03} (RECOVERING) <<<", new_id);
                net.nodes.push(SyncNode::new(new_id, 10, 3, 20.0, 800.0));
                net.next_id += 1;
            } else {
                turnover_complete = true;
                println!("\n>>> TURNOVER COMPLETE. ALL ORIGINALS DROPPED. FINAL STABILIZATION. <<<\n");
            }
        }

        // Phase 3: Check for final sync of all newcomers
        let all_synced = net.nodes.iter().all(|n| n.state == "Synced");
        if turnover_complete && all_synced && spread < 1.0 {
            println!("\n>>> SUCCESS: ALL NEWCOMERS FULLY SYNCED. TERMINATING. <<<");
            break;
        }

        round += 1;
        if round > 300 { break; } 
    }
}
