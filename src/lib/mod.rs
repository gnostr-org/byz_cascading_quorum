pub mod p2p;
#[allow(deprecated)]
use chrono::{DateTime, Duration, Timelike, Utc};
use num_bigint::BigUint;
use rand_0_8_5::{Rng as RngLegacy, thread_rng as rng_legacy};
use rand_0_9_2::{Rng as RngLatest, rng as rng_latest};
use sha2::{Digest, Sha256};
use tracing::{debug, trace};

pub use crate::p2p::evt_loop;

// --- GIT-COMPLIANT SHA-1 ENGINE ---
pub fn git_sha1(data: &[u8]) -> String {
    debug!("Starting git_sha1 for data length: {}", data.len());
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;
    let mut padded = data.to_vec();
    let bit_len = (padded.len() as u64) * 8;
    padded.push(0x80);
    while (padded.len() * 8) % 512 != 448 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }
    format!("{:08x}{:08x}{:08x}{:08x}{:08x}", h0, h1, h2, h3, h4)
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum SyncStage {
    Hour,
    Minute,
    Second,
    NonceGrind1Bit,
    NonceGrind2Bit,
    Sha256Mining,
}

pub struct SyncNode {
    pub id: usize,
    pub adjustment: Duration,
    pub stage: SyncStage,
    pub nonce: u64,
    pub success: bool,
    pub last_hash: String,
    rng_v8: rand_0_8_5::rngs::ThreadRng,
    rng_v9: rand_0_9_2::rngs::ThreadRng,
}

impl SyncNode {
    pub fn new(id: usize, offset_sec: i64) -> Self {
        debug!(
            "Creating SyncNode with id: {} and offset: {}s",
            id, offset_sec
        );
        Self {
            id,
            adjustment: Duration::seconds(offset_sec),
            stage: SyncStage::Hour,
            nonce: 0,
            success: false,
            last_hash: String::from("0000000000000000000000000000000000000000"),
            rng_v8: rng_legacy(),
            rng_v9: rng_latest(),
        }
    }

    pub fn get_logical_utc(&self) -> DateTime<Utc> {
        let logical_utc = Utc::now() + self.adjustment;
        trace!("Node {} logical UTC: {}", self.id, logical_utc);
        logical_utc
    }

    pub fn update_stage(
        &mut self,
        spread: i64,
        all_same_minute: bool,
        global_success_reached: bool,
    ) {
        debug!(
            "Node {} updating stage. Current: {:?}, Spread: {}, SameMinute: {}, GlobalSuccess: {}",
            self.id, self.stage, spread, all_same_minute, global_success_reached
        );
        match self.stage {
            SyncStage::Hour => {
                if spread < 3600 {
                    self.stage = SyncStage::Minute;
                }
            }
            SyncStage::Minute => {
                if spread < 60 {
                    self.stage = SyncStage::Second;
                }
            }
            SyncStage::Second => {
                if spread == 0 && all_same_minute {
                    self.stage = SyncStage::NonceGrind1Bit;
                }
            }
            SyncStage::NonceGrind1Bit => {
                if spread > 0 || !all_same_minute {
                    self.stage = SyncStage::Second;
                    self.success = false;
                    self.nonce = 0;
                } else if global_success_reached {
                    self.stage = SyncStage::NonceGrind2Bit;
                    self.success = false;
                }
            }
            SyncStage::NonceGrind2Bit => {
                if spread > 0 || !all_same_minute {
                    self.stage = SyncStage::Second;
                    self.success = false;
                    self.nonce = 0;
                } else if global_success_reached {
                    self.stage = SyncStage::Sha256Mining;
                    self.success = false;
                    self.nonce = 0;
                }
            }
            SyncStage::Sha256Mining => {
                if spread > 0 {
                    self.stage = SyncStage::Second;
                    self.success = false;
                    self.nonce = 0;
                }
            }
        }
    }

    pub fn grind_nonce(&mut self, target: &str) {
        debug!(
            "Node {} starting nonce grind for target: {}",
            self.id, target
        );
        let time = self.get_logical_utc();
        let minute = time.minute();
        if self.nonce == 0 {
            self.nonce = self.rng_v8.gen_range(0..1000);
        }
        loop {
            let input = format!("BLOCK-{}-{}", minute, self.nonce);
            let hash = git_sha1(input.as_bytes());
            if target == "00" {
                if hash.starts_with("00") {
                    self.last_hash = hash;
                    self.success = true;
                    break;
                } else if hash.starts_with('0') {
                    self.last_hash = hash;
                    self.nonce += 1;
                    break;
                }
            } else {
                self.last_hash = hash;
                if self.last_hash.starts_with(target) {
                    self.success = true;
                } else {
                    self.nonce += 1;
                }
                break;
            }
            self.nonce += 1;
        }
    }

    pub fn mine_sha256(&mut self, target: &str) {
        debug!(
            "Node {} starting SHA-256 mining for target: {}",
            self.id, target
        );
        let mut bytes = [0u8; 64];
        self.rng_v9.fill(&mut bytes);
        let candidate = BigUint::from_bytes_be(&bytes);
        loop {
            let input = format!("{}-{}-{}", self.last_hash, self.nonce, candidate);
            let mut hasher = Sha256::new();
            hasher.update(input.as_bytes());
            let hash = format!("{:x}", hasher.finalize());
            if hash.starts_with(target) {
                self.last_hash = hash;
                self.success = true;
                break;
            } else {
                self.last_hash = hash; // Still update last_hash for progress visibility
                self.nonce += 1;
            }
        }
    }
}

pub fn get_median_diff(timestamps: &[i64], current: i64) -> i64 {
    trace!("Calculating median diff for current: {}", current);
    let mut diffs: Vec<i64> = timestamps.iter().map(|t| t - current).collect();
    diffs.sort();
    diffs[diffs.len() / 2]
}

pub fn print_report_header(round: i32, nodes_count: usize, spread: i64) {
    if log::log_enabled!(log::Level::Info) {
        println!(
            "
ROUND: {:03} | NODES: {:02} | SPREAD: {}s",
            round, nodes_count, spread
        );
        println!("{:-<85}", "");
        println!(
            "{:<4} | {:<15} | {:<12} | {:<8} | {:<6} | {:<64}",
            "ID", "STAGE", "LOGICAL UTC", "NONCE", "STATUS", "LAST HASH (TRUNC)"
        );
        println!("{:-<85}", "");
    }
}

pub fn run_byz_cascading_quorum_v2(difficulty: u8) {
    debug!("Starting Byz Cascading Quorum V2 simulation with difficulty: {}.", difficulty);
    let mut nodes: Vec<SyncNode> = (0..5).map(|i| SyncNode::new(i, i as i64 * 2)).collect();
    let mut round = 1;
    let mut entrants_joined = false;

    let target_difficulty_str = "0".repeat(difficulty as usize);

    println!("--- [DISTRIBUTED CONSENSUS REPORT] ---");

    loop {
        let current_times: Vec<DateTime<Utc>> = nodes.iter().map(|n| n.get_logical_utc()).collect();
        let timestamps: Vec<i64> = current_times.iter().map(|t| t.timestamp()).collect();
        let spread = (timestamps.iter().max().unwrap() - timestamps.iter().min().unwrap()).abs();
        let all_same_minute = current_times
            .iter()
            .all(|t| t.minute() == current_times[0].minute());
        let global_success_reached = nodes.iter().all(|n| n.success);

        // TRIGGER: Entry gated by 2-bit SHA256 completion
        let first_nodes_finished = nodes.len() == 5
            && nodes
                .iter()
                .all(|n| n.stage == SyncStage::Sha256Mining && n.success);

        if !entrants_joined && first_nodes_finished {
            println!(
                "
>>> EVENT: 2-BIT CONSENSUS REACHED. JOINING 3 NODES & ESCALATING TO 3-BIT TARGET <<<"
            );
            let start_id = nodes.len();
            for i in 0..3 {
                nodes.push(SyncNode::new(start_id + i, 1200 + (i as i64 * 15)));
            }
            entrants_joined = true;
            for node in &mut nodes {
                node.success = false;
            }
            continue;
        }

        if round == 1 || round % 5 == 0 || (entrants_joined && round < 250) {
            if log::log_enabled!(log::Level::Info) {
                print_report_header(round, nodes.len(), spread);
            }
        }

        for i in 0..nodes.len() {
            nodes[i].update_stage(spread, all_same_minute, global_success_reached);
            match nodes[i].stage {
                SyncStage::Hour | SyncStage::Minute | SyncStage::Second => {
                    let d = get_median_diff(&timestamps, timestamps[i]);
                    let step = if nodes[i].stage == SyncStage::Second {
                        d.signum()
                    } else {
                        d / 2
                    };
                    nodes[i].adjustment = nodes[i].adjustment + Duration::seconds(step);
                }
                SyncStage::NonceGrind1Bit => {
                    if !nodes[i].success {
                        nodes[i].grind_nonce("0");
                    }
                }
                SyncStage::NonceGrind2Bit => {
                    if !nodes[i].success {
                        nodes[i].grind_nonce("00");
                    }
                }
                SyncStage::Sha256Mining => {
                    if !nodes[i].success {
                        let target = if entrants_joined {
                            &target_difficulty_str[..]
                        } else {
                            "000"
                        };
                        nodes[i].mine_sha256(target);
                    }
                }
            };

            if round == 1 || round % 5 == 0 || (entrants_joined && round < 250) {
                let status = if nodes[i].success { "SOLVED" } else { "---" };
                if nodes[i].stage == SyncStage::NonceGrind2Bit || nodes[i].stage == SyncStage::NonceGrind1Bit {
                    debug!(
                        "{:02}   | {:<15?} | {:<12} | {:<8} | {:<6} | {:<64}",
                        nodes[i].id,
                        nodes[i].stage,
                        current_times[i].format("%H:%M:%S"),
                        nodes[i].nonce,
                        status,
                        &nodes[i].last_hash
                    );
                } else {
                    println!(
                        "{:02}   | {:<15?} | {:<12} | {:<8} | {:<6} | {:<64}",
                        nodes[i].id,
                        nodes[i].stage,
                        current_times[i].format("%H:%M:%S"),
                        nodes[i].nonce,
                        status,
                        &nodes[i].last_hash
                    );
                }
            }
        }

        // EXIT: Quorum must achieve `difficulty`-bit finality
        if entrants_joined
            && nodes.iter().all(|n| {
                n.stage == SyncStage::Sha256Mining
                    && n.success
                    && n.last_hash.starts_with(&target_difficulty_str[..])
            })
        {
            println!("{:-<85}", "");
            println!(
                ">>> CONSENSUS FINALIZED ACROSS ALL {} NODES AT {}-BIT DIFFICULTY <<<",
                nodes.len(),
                difficulty
            );
            break;
        }

        round += 1;
        if round > 15000 {
            break;
        } // Increased limit to account for 3-bit difficulty time
    }
}

pub fn run_byz_cascading_quorum() {
    debug!("Starting Byz Cascading Quorum simulation.");
    let mut nodes: Vec<SyncNode> = (0..10)
        .map(|i| {
            let offset = match i {
                0..=2 => 10,
                3..=5 => 5,
                _ => 2,
            };
            SyncNode::new(i, offset)
        })
        .collect();

    let mut round = 1;
    loop {
        let current_times: Vec<DateTime<Utc>> = nodes.iter().map(|n| n.get_logical_utc()).collect();
        let timestamps: Vec<i64> = current_times.iter().map(|t| t.timestamp()).collect();
        let spread = (timestamps.iter().max().unwrap() - timestamps.iter().min().unwrap()).abs();
        let all_same_minute = current_times
            .iter()
            .all(|t| t.minute() == current_times[0].minute());

        let global_step_reached = nodes.iter().all(|n| n.success);

        println!(
            "
--- [ROUND {:03}] Spread:{}s | MinSync:{} | Phase:{:?} ---",
            round, spread, all_same_minute, nodes[0].stage
        );

        for i in 0..10 {
            nodes[i].update_stage(spread, all_same_minute, global_step_reached);
            match nodes[i].stage {
                SyncStage::Hour | SyncStage::Minute | SyncStage::Second => {
                    let d = get_median_diff(&timestamps, timestamps[i]);
                    let step = if nodes[i].stage == SyncStage::Second {
                        d.signum()
                    } else {
                        d / 2
                    };
                    nodes[i].adjustment = nodes[i].adjustment + Duration::seconds(step);
                }
                SyncStage::NonceGrind1Bit => {
                    if !nodes[i].success {
                        nodes[i].grind_nonce("0");
                    }
                }
                SyncStage::NonceGrind2Bit => {
                    if !nodes[i].success {
                        nodes[i].grind_nonce("00");
                    }
                }
                SyncStage::Sha256Mining => {
                    if !nodes[i].success {
                        nodes[i].mine_sha256("00");
                    }
                }
            };

            let mark = if nodes[i].success {
                "SOLVED "
            } else {
                "WAITING"
            };
            println!(
                "N{:02}|S:{}|UTC:{}|Nonce:{:<6}|{}|HASH:{}",
                i,
                nodes[i].stage as u8,
                current_times[i].format("%H:%M:%S"),
                nodes[i].nonce,
                mark,
                nodes[i].last_hash
            );
        }

        if nodes
            .iter()
            .all(|n| n.stage == SyncStage::Sha256Mining && n.success)
        {
            println!(
                "
>>> SUCCESS: ALL NODES ATTAINED SHA-256 2-BIT FINALITY <<<"
            );
            break;
        }
        round += 1;
        if round > 10000 {
            break;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EstimationTime {
    pub d: f64,
    pub a: f64,
}

pub fn estimate_offset_time(s: f64, r: f64, c: f64) -> EstimationTime {
    trace!("Estimating offset time for s: {}, r: {}, c: {}", s, r, c);
    EstimationTime {
        d: c - (r + s) / 2.0,
        a: (r - s) / 2.0,
    }
}

pub struct SyncNodeTime {
    pub id: usize,
    pub n: usize,
    pub f: usize,
    pub way_off: f64,
    pub adj_p: f64,
    pub state: String,
}

impl SyncNodeTime {
    pub fn new(id: usize, n: usize, f: usize, way_off: f64, initial_adj: f64) -> Self {
        debug!(
            "Creating SyncNodeTime id: {}, n: {}, f: {}, way_off: {}, initial_adj: {}",
            id, n, f, way_off, initial_adj
        );
        Self {
            id,
            n,
            f,
            way_off,
            adj_p: initial_adj,
            state: String::from("Init"),
        }
    }

    pub fn get_local_time(&self) -> f64 {
        trace!("NodeTime {} local time: {}", self.id, self.adj_p);
        self.adj_p
    }

    pub fn run_sync_cycle(&mut self, estimates: Vec<EstimationTime>) {
        debug!(
            "NodeTime {} running sync cycle with {} estimates.",
            self.id,
            estimates.len()
        );
        let mut d_overs: Vec<f64> = estimates.iter().map(|e| e.d + e.a).collect();
        let mut d_unders: Vec<f64> = estimates.iter().map(|e| e.d - e.a).collect();

        d_overs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        d_unders.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let m = d_overs[self.f];
        let m_large = d_unders[self.n - 1 - self.f];

        let raw_adjustment = if m > -self.way_off && m_large < self.way_off {
            self.state = String::from("🟢");
            (m.min(0.0) + m_large.max(0.0)) / 2.0
        } else {
            self.state = String::from("🟡");
            (m + m_large) / 2.0
        };

        // 40% correction to visualize the halving convergence property
        self.adj_p += raw_adjustment * 0.4;
    }
}

pub struct NetworkTime {
    pub nodes: Vec<SyncNodeTime>,
    pub next_id: usize,
}

impl NetworkTime {
    pub fn new() -> Self {
        debug!("Creating new NetworkTime.");
        let mut nodes = Vec::new();
        for i in 0..10 {
            let offset = if i < 4 { 1000.0 } else { 0.0 };
            nodes.push(SyncNodeTime::new(i, 10, 3, 20.0, offset));
        }
        NetworkTime { nodes, next_id: 10 }
    }

    pub fn simulate_step(&mut self) -> f64 {
        debug!("Simulating network time step.");
        let current_n = self.nodes.len();
        let mut all_estimates = Vec::new();

        for p_idx in 0..current_n {
            let mut p_estimates = Vec::new();
            let s = self.nodes[p_idx].get_local_time();
            for q_idx in 0..current_n {
                let c = self.nodes[q_idx].get_local_time();
                let r = self.nodes[p_idx].get_local_time();
                p_estimates.push(estimate_offset_time(s, r, c));
            }
            all_estimates.push(p_estimates);
        }

        let mut min_t = f64::MAX;
        let mut max_t = f64::MIN;

        for i in 0..current_n {
            let estimates = all_estimates[i].clone();
            self.nodes[i].run_sync_cycle(estimates);
            let time = self.nodes[i].get_local_time();
            if time < min_t {
                min_t = time;
            }
            if time > max_t {
                max_t = time;
            }
            println!(
                "Node {:03} | {:11} | Clock: {:7.2}",
                self.nodes[i].id, self.nodes[i].state, time
            );
        }
        max_t - min_t
    }
}

pub fn run_byz_time() {
    debug!("Starting Byz Time simulation.");
    let mut net = NetworkTime::new();
    let mut round = 1;
    let mut turnover_started = false;
    let mut turnover_complete = false;

    loop {
        println!("--- Round {:02} ---", round);
        let spread = net.simulate_step();

        // Phase 1 -> Phase 2 Transition
        if spread < 1.0 && !turnover_started {
            println!(
                "
>>> SYNC REACHED. COMMENCING TOTAL TURNOVER. <<<
"
            );
            turnover_started = true;
        }

        // Phase 2: Dropping originals
        if turnover_started && !turnover_complete {
            let to_drop = net.nodes.iter().find(|n| n.id < 10).map(|n| n.id);
            if let Some(id) = to_drop {
                println!(">>> DROPPING ORIGINAL Node {:03} <<<", id);
                net.nodes.retain(|n| n.id != id);

                let new_id = net.next_id;
                println!(">>> ADDING NEWCOMER Node {:03} (🟡) <<<", new_id);
                net.nodes
                    .push(SyncNodeTime::new(new_id, 10, 3, 20.0, 800.0));
                net.next_id += 1;
            } else {
                turnover_complete = true;
                println!(
                    "
>>> TURNOVER COMPLETE. ALL ORIGINALS DROPPED. FINAL STABILIZATION. <<<
"
                );
            }
        }

        // Phase 3: Check for final sync of all newcomers
        let all_synced = net.nodes.iter().all(|n| n.state == "🟢");
        if turnover_complete && all_synced && spread < 1.0 {
            println!(
                "
>>> SUCCESS: ALL NEWCOMERS FULLY SYNCED. TERMINATING. <<<"
            );
            break;
        }

        round += 1;
        if round > 300 {
            break;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EstimationUtc {
    pub d: f64, // Difference in seconds
    pub a: f64, // Uncertainty
}

pub fn estimate_offset_utc(s: DateTime<Utc>, r: DateTime<Utc>, c: DateTime<Utc>) -> EstimationUtc {
    trace!(
        "Estimating UTC offset for s: {:?}, r: {:?}, c: {:?}",
        s, r, c
    );
    // d = c - (r + s) / 2
    let send_receive_avg_ms = (r.timestamp_millis() + s.timestamp_millis()) / 2;
    let diff_ms = c.timestamp_millis() - send_receive_avg_ms;

    EstimationUtc {
        d: diff_ms as f64 / 1000.0,
        a: (r.timestamp_millis() - s.timestamp_millis()) as f64 / 2000.0,
    }
}

pub struct SyncNodeUtc {
    pub id: usize,
    pub n: usize,
    pub f: usize,
    pub way_off: f64,
    pub adjustment: Duration,
    pub state: String,
}

impl SyncNodeUtc {
    pub fn new(id: usize, n: usize, f: usize, way_off: f64, initial_offset_sec: i64) -> Self {
        debug!(
            "Creating SyncNodeUtc id: {}, n: {}, f: {}, way_off: {}, initial_offset_sec: {}",
            id, n, f, way_off, initial_offset_sec
        );
        Self {
            id,
            n,
            f,
            way_off,
            adjustment: Duration::seconds(initial_offset_sec),
            state: String::from("⚪️"),
        }
    }

    pub fn get_logical_utc(&self) -> DateTime<Utc> {
        let logical_utc = Utc::now() + self.adjustment;
        trace!("Node {} logical UTC: {}", self.id, logical_utc);
        logical_utc
    }

    pub fn run_sync_cycle(&mut self, estimates: Vec<EstimationUtc>) {
        let mut d_overs: Vec<f64> = estimates.iter().map(|e| e.d + e.a).collect();
        let mut d_unders: Vec<f64> = estimates.iter().map(|e| e.d - e.a).collect();

        d_overs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        d_unders.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let m = d_overs[self.f];
        let m_large = d_unders[self.n - 1 - self.f];

        let raw_adj_sec = if m > -self.way_off && m_large < self.way_off {
            self.state = String::from("🟢");
            (m.min(0.0) + m_large.max(0.0)) / 2.0
        } else {
            self.state = String::from("🟡");
            (m + m_large) / 2.0
        };

        // Apply 80% of the correction (halving-style convergence)
        let apply_ms = (raw_adj_sec * 800.0) as i64;
        self.adjustment = self.adjustment + Duration::milliseconds(apply_ms);
    }
}

pub struct NetworkUtc {
    pub nodes: Vec<SyncNodeUtc>,
    pub next_id: usize,
}

impl NetworkUtc {
    pub fn new() -> Self {
        debug!("Creating new NetworkUtc.");
        let mut nodes = Vec::new();
        for i in 0..10 {
            // Original nodes 0-3 are 1 hour (3600s) ahead
            let offset = if i < 4 { 3600 } else { 0 };
            nodes.push(SyncNodeUtc::new(i, 10, 3, 20.0, offset));
        }
        NetworkUtc { nodes, next_id: 10 }
    }

    pub fn simulate_step(&mut self) -> f64 {
        debug!("Simulating network time step.");
        let current_n = self.nodes.len();
        let mut all_estimates = Vec::new();

        for p_idx in 0..current_n {
            let mut p_estimates = Vec::new();
            let s = self.nodes[p_idx].get_logical_utc();
            for q_idx in 0..current_n {
                let c = self.nodes[q_idx].get_logical_utc();
                let r = self.nodes[p_idx].get_logical_utc();
                p_estimates.push(estimate_offset_utc(s, r, c));
            }
            all_estimates.push(p_estimates);
        }

        let mut timestamps: Vec<i64> = Vec::new();
        for i in 0..current_n {
            let estimates = all_estimates[i].clone();
            self.nodes[i].run_sync_cycle(estimates);
            let time = self.nodes[i].get_logical_utc();
            timestamps.push(time.timestamp_millis());
            println!(
                "Node {:03} | {:11} | UTC: {}",
                self.nodes[i].id,
                self.nodes[i].state,
                time.format("%H:%M:%S%.3f")
            );
        }

        let min_t = *timestamps.iter().min().unwrap();
        let max_t = *timestamps.iter().max().unwrap();
        (max_t - min_t) as f64 / 1000.0
    }
}

pub fn run_utc_consensus() {
    debug!("Starting UTC consensus simulation.");
    let mut net = NetworkUtc::new();
    let mut round = 1;
    let mut turnover_started = false;
    let mut turnover_complete = false;

    loop {
        println!("--- Round {:02} ---", round);
        let spread_sec = net.simulate_step();

        if spread_sec < 1.0 && !turnover_started {
            println!(
                "
>>> UTC SYNC REACHED. COMMENCING TOTAL TURNOVER. <<<
"
            );
            turnover_started = true;
        }

        if turnover_started && !turnover_complete {
            let to_drop = net.nodes.iter().find(|n| n.id < 10).map(|n| n.id);
            if let Some(id) = to_drop {
                println!(">>> DROPPING ORIGINAL Node {:03} <<<", id);
                net.nodes.retain(|n| n.id != id);

                let new_id = net.next_id;
                // Newcomer starts 2 hours (7200s) behind
                println!(">>> ADDING NEWCOMER Node {:03} (🟡) <<<", new_id);
                net.nodes.push(SyncNodeUtc::new(new_id, 10, 3, 20.0, -7200));
                net.next_id += 1;
            } else {
                turnover_complete = true;
                println!(
                    "
>>> TURNOVER COMPLETE. WAITING FOR NEWCOMER SYNC. <<<
"
                );
            }
        }

        let all_synced = net.nodes.iter().all(|n| n.state == "🟢");
        if turnover_complete && all_synced && spread_sec < 1.0 {
            println!(
                "
>>> SUCCESS: ALL NEWCOMERS SYNCED TO UTC CONSENSUS. <<<"
            );
            break;
        }

        round += 1;
        if round > 500 {
            break;
        }
    }
}
