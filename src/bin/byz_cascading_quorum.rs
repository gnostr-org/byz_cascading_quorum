use chrono::{DateTime, Duration, Utc, Timelike};
use sha2::{Sha256, Digest};

// --- GIT-COMPLIANT SHA-1 ENGINE ---
fn git_sha1(data: &[u8]) -> String {
    let mut h0: u32 = 0x67452301; let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE; let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;
    let mut padded = data.to_vec();
    let bit_len = (padded.len() as u64) * 8;
    padded.push(0x80);
    while (padded.len() * 8) % 512 != 448 { padded.push(0); }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 { w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]); }
        for i in 16..80 { w[i] = (w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16]).rotate_left(1); }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d; d = c; c = b.rotate_left(30); b = a; a = temp;
        }
        h0 = h0.wrapping_add(a); h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c); h3 = h3.wrapping_add(d); h4 = h4.wrapping_add(e);
    }
    format!("{:08x}{:08x}{:08x}{:08x}{:08x}", h0, h1, h2, h3, h4)
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum SyncStage { Hour, Minute, Second, NonceGrind1Bit, NonceGrind2Bit, Sha256Mining }

pub struct SyncNode {
    pub id: usize,
    pub adjustment: Duration,
    pub stage: SyncStage,
    pub nonce: u64,
    pub success: bool,
    pub last_hash: String,
}

impl SyncNode {
    pub fn new(id: usize, offset_sec: i64) -> Self {
        Self {
            id,
            adjustment: Duration::seconds(offset_sec),
            stage: SyncStage::Hour,
            nonce: 0,
            success: false,
            last_hash: String::from("0000000000000000000000000000000000000000"),
        }
    }

    pub fn get_logical_utc(&self) -> DateTime<Utc> {
        Utc::now() + self.adjustment
    }

    pub fn update_stage(&mut self, spread: i64, all_same_minute: bool, global_success_reached: bool) {
        match self.stage {
            SyncStage::Hour => if spread < 3600 { self.stage = SyncStage::Minute; },
            SyncStage::Minute => if spread < 60 { self.stage = SyncStage::Second; },
            SyncStage::Second => if spread == 0 && all_same_minute { self.stage = SyncStage::NonceGrind1Bit; },
            SyncStage::NonceGrind1Bit => {
                if spread > 0 || !all_same_minute {
                    self.stage = SyncStage::Second;
                    self.success = false;
                    self.nonce = 0;
                } else if global_success_reached {
                    self.stage = SyncStage::NonceGrind2Bit;
                    self.success = false;
                }
            },
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
            },
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
        let time = self.get_logical_utc();
        let minute = time.minute();
        
        loop {
            let input = format!("BLOCK-{}-{}", minute, self.nonce);
            let hash = git_sha1(input.as_bytes());
            
            if target == "00" {
                if hash.starts_with("000") {
                    self.last_hash = hash;
                    self.success = true;
                    break;
                } else if hash.starts_with("0") {
                    self.last_hash = hash;
                    self.nonce += 1;
                    break;
                }
            } else {
                self.last_hash = hash;
                if self.last_hash.starts_with(target) { self.success = true; } else { self.nonce += 1; }
                break;
            }
            self.nonce += 1;
        }
    }

    pub fn mine_sha256(&mut self, target: &str) {
        loop {
            // We use the last hash as a base (which is the SHA-1 consensus hash when the phase starts)
            let input = format!("{}-{}", self.last_hash, self.nonce);
            let mut hasher = Sha256::new();
            hasher.update(input.as_bytes());
            let hash = format!("{:x}", hasher.finalize());

            // 1-bit minimum: Only update last_hash and exit loop if it starts with '0'
            if hash.starts_with('0') {
                if hash.starts_with(target) {
                    self.last_hash = hash;
                    self.success = true;
                } else {
                    self.last_hash = hash;
                    // If we found a 1-bit but not the 2-bit target, increment nonce 
                    // for the next iteration call to ensure we don't repeat this hash.
                    self.nonce += 1;
                }
                break;
            }
            self.nonce += 1;
        }
    }
}

fn get_median_diff(timestamps: &[i64], current: i64) -> i64 {
    let mut diffs: Vec<i64> = timestamps.iter().map(|t| t - current).collect();
    diffs.sort();
    diffs[diffs.len() / 2]
}

fn main() {
    let mut nodes: Vec<SyncNode> = (0..10).map(|i| {
        let offset = match i { 0..=2 => 10, 3..=5 => 5, _ => 2 };
        SyncNode::new(i, offset)
    }).collect();

    let mut round = 1;
    loop {
        let current_times: Vec<DateTime<Utc>> = nodes.iter().map(|n| n.get_logical_utc()).collect();
        let timestamps: Vec<i64> = current_times.iter().map(|t| t.timestamp()).collect();
        let spread = (timestamps.iter().max().unwrap() - timestamps.iter().min().unwrap()).abs();
        let all_same_minute = current_times.iter().all(|t| t.minute() == current_times[0].minute());
        
        let global_step_reached = nodes.iter().all(|n| n.success);

        println!("\n--- [ROUND {:03}] Spread:{}s | MinSync:{} | Phase:{:?} ---", round, spread, all_same_minute, nodes[0].stage);

        for i in 0..10 {
            nodes[i].update_stage(spread, all_same_minute, global_step_reached);
            match nodes[i].stage {
                SyncStage::Hour | SyncStage::Minute | SyncStage::Second => {
                    let d = get_median_diff(&timestamps, timestamps[i]);
                    let step = if nodes[i].stage == SyncStage::Second { d.signum() } else { d / 2 };
                    nodes[i].adjustment = nodes[i].adjustment + Duration::seconds(step);
                },
                SyncStage::NonceGrind1Bit => { if !nodes[i].success { nodes[i].grind_nonce("0"); } }
                SyncStage::NonceGrind2Bit => { if !nodes[i].success { nodes[i].grind_nonce("00"); } }
                SyncStage::Sha256Mining => { if !nodes[i].success { nodes[i].mine_sha256("00"); } }
            };

            let mark = if nodes[i].success { "SOLVED " } else { "WAITING" };
            println!(
                "N{:02}|S:{}|UTC:{}|Nonce:{:<6}|{}|HASH:{}", 
                i, nodes[i].stage as u8, current_times[i].format("%H:%M:%S"), 
                nodes[i].nonce, mark, nodes[i].last_hash
            );
        }

        if nodes.iter().all(|n| n.stage == SyncStage::Sha256Mining && n.success) {
            println!("\n>>> SUCCESS: ALL NODES ATTAINED SHA-256 2-BIT FINALITY <<<");
            break;
        }
        round += 1;
        if round > 10000 { break; }
    }
}
