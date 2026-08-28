use byz_time::{
    SyncNodeUtc, estimate_offset_utc,
    p2p::{MessageReassembler, Msg, MsgKind},
};
use chrono::{Duration, TimeZone, Utc};

fn chunk(message_id: &str, sequence_num: usize, total_chunks: usize, content: &str) -> Msg {
    Msg {
        from: Some("peer-0".to_string()),
        kind: MsgKind::TimeSync,
        commit_id: Some("commit-1".to_string()),
        nostr_event: Some("event-1".to_string()),
        message_id: Some(message_id.to_string()),
        sequence_num: Some(sequence_num),
        total_chunks: Some(total_chunks),
        content: vec![content.to_string()],
    }
}

#[tokio::test]
async fn message_reassembler_requires_chunk_metadata() {
    let reassembler = MessageReassembler::new();
    let msg = Msg {
        content: vec!["missing metadata".to_string()],
        ..Msg::default()
    };

    let reassembled = reassembler.add_chunk_and_reassemble(msg).await;

    assert!(reassembled.is_none());
}

#[tokio::test]
async fn message_reassembler_reassembles_out_of_order_chunks() {
    let reassembler = MessageReassembler::new();

    assert!(
        reassembler
            .add_chunk_and_reassemble(chunk("msg-1", 2, 3, "world"))
            .await
            .is_none()
    );
    assert!(
        reassembler
            .add_chunk_and_reassemble(chunk("msg-1", 0, 3, "hello "))
            .await
            .is_none()
    );

    let reassembled = reassembler
        .add_chunk_and_reassemble(chunk("msg-1", 1, 3, "async "))
        .await
        .expect("final chunk should reassemble the message");

    assert_eq!(reassembled.from.as_deref(), Some("peer-0"));
    assert_eq!(reassembled.kind, MsgKind::TimeSync);
    assert_eq!(reassembled.commit_id.as_deref(), Some("commit-1"));
    assert_eq!(reassembled.nostr_event.as_deref(), Some("event-1"));
    assert_eq!(reassembled.content, vec!["hello async world".to_string()]);
    assert!(reassembled.message_id.is_none());
    assert!(reassembled.sequence_num.is_none());
    assert!(reassembled.total_chunks.is_none());
}

#[tokio::test]
async fn message_reassembler_ignores_duplicate_chunks() {
    let reassembler = MessageReassembler::new();

    assert!(
        reassembler
            .add_chunk_and_reassemble(chunk("msg-2", 0, 2, "hello "))
            .await
            .is_none()
    );
    assert!(
        reassembler
            .add_chunk_and_reassemble(chunk("msg-2", 0, 2, "hello "))
            .await
            .is_none()
    );

    let reassembled = reassembler
        .add_chunk_and_reassemble(chunk("msg-2", 1, 2, "world"))
        .await
        .expect("unique final chunk should reassemble the message");

    assert_eq!(reassembled.content, vec!["hello world".to_string()]);
}

#[test]
fn estimate_offset_utc_uses_midpoint_and_uncertainty() {
    let s = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let r = s + Duration::seconds(2);
    let c = s + Duration::seconds(4);

    let estimate = estimate_offset_utc(s, r, c);

    assert_eq!(estimate.d, 3.0);
    assert_eq!(estimate.a, 1.0);
}

#[test]
fn sync_node_utc_applies_bounded_green_state_adjustment() {
    let mut node = SyncNodeUtc::new(0, 4, 1, 30.0, 0);

    node.run_sync_cycle(vec![
        byz_time::EstimationUtc { d: 1.0, a: 0.0 },
        byz_time::EstimationUtc { d: 1.0, a: 0.0 },
        byz_time::EstimationUtc { d: 1.0, a: 0.0 },
        byz_time::EstimationUtc { d: 1.0, a: 0.0 },
    ]);

    assert_eq!(node.state, "🟢");
    assert_eq!(node.adjustment, Duration::milliseconds(1000));
}

#[test]
fn sync_node_utc_treats_negative_way_off_boundary_as_bounded() {
    let mut node = SyncNodeUtc::new(0, 4, 1, 1.0, 0);

    node.run_sync_cycle(vec![
        byz_time::EstimationUtc { d: -2.0, a: 0.0 },
        byz_time::EstimationUtc { d: -1.0, a: 0.0 },
        byz_time::EstimationUtc { d: 0.0, a: 0.0 },
        byz_time::EstimationUtc { d: 1.0, a: 0.0 },
    ]);

    assert_eq!(node.state, "🟢");
}
