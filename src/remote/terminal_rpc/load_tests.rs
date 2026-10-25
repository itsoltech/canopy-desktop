//! Component load evidence only. No LAN, native client apply/render, GUI or
//! ten-minute AC-30 claim; uses isolated SQLite and generated 1 KiB deltas.
use crate::{
    remote::sync::{Error, Limits, ReplayHub, Start},
    settings::{ChatHistoryBatch, RemoteHistoryRequest, SettingsClient},
};
use std::time::{Duration, Instant};

#[tokio::test]
async fn eight_devices_bulk_and_nonreader_preserve_fanout_and_100k_history_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let db = SettingsClient::create(&dir.path().join("history.db"))
        .await
        .unwrap();
    let chat = uuid::Uuid::new_v4().to_string();
    for batch in 0..100u64 {
        db.save_chat_history(vec![ChatHistoryBatch {
            chat: chat.clone(),
            epochs: vec![],
            items: (1..=1000)
                .map(|i| {
                    let id = batch * 1000 + i;
                    (0, id, format!("history {id} żółw"))
                })
                .collect(),
        }])
        .await
        .unwrap();
    }
    let first = db
        .read_remote_history_page(RemoteHistoryRequest {
            chat: chat.clone(),
            before: None,
            upper: None,
            limit: 100,
        })
        .await
        .unwrap();
    assert_eq!(first.rows.items.len(), 100);
    assert_eq!(first.upper, Some((0, 100_000)));
    let hub = ReplayHub::new(Limits::default());
    for session in 0..8 {
        hub.open_stream(&format!("session-{session}")).unwrap();
    }
    let mut subscribers = Vec::new();
    for device in 0..8 {
        for view in 0..4 {
            let stream = format!("session-{}", (device * 4 + view) % 8);
            let (_, subscription) = hub
                .subscribe(&stream, &format!("device-{device}"), None)
                .unwrap();
            subscribers.push((device, subscription));
        }
    }
    let history = async {
        let mut page = first;
        let upper = page.upper;
        let mut expected = 100_000;
        let mut pages = 0;
        loop {
            assert!(page.rows.items.len() <= 100);
            assert!(
                page.rows
                    .items
                    .iter()
                    .map(|(_, _, text)| text.len())
                    .sum::<usize>()
                    <= 1024 * 1024
            );
            for (_, id, _) in page.rows.items.iter().rev() {
                assert_eq!(*id, expected);
                expected -= 1;
            }
            pages += 1;
            let Some(before) = page.next_before else {
                break;
            };
            page = db
                .read_remote_history_page(RemoteHistoryRequest {
                    chat: chat.clone(),
                    before: Some(before),
                    upper,
                    limit: 100,
                })
                .await
                .unwrap();
        }
        assert_eq!(expected, 0);
        assert_eq!(pages, 1000);
    };
    let fanout = async {
        let slow_bulk = hub.reserve_output("device-0", 64 * 1024).unwrap();
        let mut peak = 0;
        let mut timings = Vec::new();
        let mut interval = tokio::time::interval(Duration::from_millis(10));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        for tick in 0..300u64 {
            interval.tick().await;
            let mut bulk = Vec::new();
            for device in 1..8 {
                bulk.push(
                    hub.reserve_output(&format!("device-{device}"), 64 * 1024)
                        .unwrap(),
                );
            }
            for session in 0..8 {
                let now = Instant::now();
                hub.publish(
                    &format!("session-{session}"),
                    vec![(tick % 256) as u8; 1024],
                )
                .unwrap();
                timings.push(now.elapsed());
            }
            peak = peak.max(hub.queued_bytes());
            assert!(hub.queued_bytes() <= 32 * 1024 * 1024);
            assert!(hub.ring_bytes() <= 16 * 1024 * 1024);
            for (device, subscription) in &subscribers {
                if *device == 0 {
                    continue;
                }
                let event = subscription.try_recv().unwrap().unwrap();
                assert_eq!(event.cursor().sequence(), tick + 1);
                assert_eq!(event.payload(), vec![(tick % 256) as u8; 1024]);
            }
            // Concurrent persisted writes cannot move the paginated boundary.
            if tick % 100 == 0 {
                db.save_chat_history(vec![ChatHistoryBatch {
                    chat: chat.clone(),
                    epochs: vec![],
                    items: vec![(0, 100_001 + tick, "live storage mutation".into())],
                }])
                .await
                .unwrap();
            }
            drop(bulk);
        }
        for (_, subscription) in subscribers.iter().filter(|(device, _)| *device == 0) {
            assert_eq!(subscription.check(), Err(Error::ResyncRequired));
        }
        drop(slow_bulk);
        let (snapshot, recovered) = hub.subscribe("session-0", "device-0", None).unwrap();
        let Start::Snapshot { boundary } = snapshot else {
            panic!("fresh snapshot required");
        };
        let after = hub.publish("session-0", b"recovered".to_vec()).unwrap();
        let delta = recovered.try_recv().unwrap().unwrap();
        assert_eq!(after.sequence(), boundary.sequence() + 1);
        assert_eq!(delta.cursor(), &after);
        assert_eq!(delta.payload(), b"recovered");
        timings.sort_unstable();
        eprintln!(
            "component load: 8 devices, 32 subscriptions, 8 sessions x 100 deltas/s x 3s, 2400 x 1KiB publish samples, 7 concurrent 64KiB bulk chunks, queue peak {peak} bytes, publish p95 {:?}; native AC-30 NOT RUN",
            timings[timings.len() * 95 / 100]
        );
    };
    tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(history, fanout);
    })
    .await
    .unwrap();
    drop(subscribers);
    assert_eq!(hub.queued_bytes(), 0);
}
