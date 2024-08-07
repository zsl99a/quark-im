use std::{collections::BTreeMap, sync::Arc, time::Duration};

use dashmap::DashMap;
use uuid::Uuid;

pub struct RoutingQueryTask {
    peer_id: Uuid,
    report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>,
    paths: Arc<DashMap<Uuid, (Vec<Uuid>, u64)>>,
}

impl RoutingQueryTask {
    pub fn new(peer_id: Uuid, report: Arc<DashMap<Uuid, BTreeMap<Uuid, u64>>>, paths: Arc<DashMap<Uuid, (Vec<Uuid>, u64)>>) -> Self {
        Self { peer_id, report, paths }
    }
}

impl RoutingQueryTask {
    pub async fn future(self) {
        tokio::time::sleep(Duration::from_secs(5)).await;

        let mut interval = tokio::time::interval(Duration::from_secs(3));

        loop {
            let report = self.get_report();

            for target_id in self.report.iter() {
                let path = pathfinding::prelude::dijkstra(
                    &self.peer_id,
                    |x| {
                        report
                            .get(x)
                            .map(|x| {
                                x.iter()
                                    .map(|(k, v)| {
                                        // 多走一层节点需要增加一定数量的预估延迟，认为是系统内部的损耗 (µs)
                                        (*k, v + 1000)
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    },
                    |p| p == target_id.key(),
                );

                if let Some(path) = path {
                    self.paths.insert(*target_id.key(), path);
                }
            }

            self.paths.retain(|key, _| self.report.get(key).is_some());

            interval.tick().await;
        }
    }

    pub fn get_report(&self) -> BTreeMap<Uuid, BTreeMap<Uuid, u64>> {
        let mut report = BTreeMap::new();
        for item in self.report.iter() {
            report.insert(*item.key(), item.value().clone());
        }

        // let mut speeds = BTreeMap::new();
        // for item in self.speeds.iter() {
        //     speeds.insert(*item.key(), *item.value());
        // }
        // report.insert(self.peer_id, speeds);

        report
    }
}
