use super::ledger::Ledger;
use super::ledger_wire::LedgerHello;
use super::proxy_ledger::ProxyLedger;
use super::queue::LocalQueue;
use super::queue_replay::{self, Replay};
use crate::config::ProxyConfig;
use crate::store::persistence::{PersistHandle, SchemaPolicy};

pub enum Reach {
    Database(PersistHandle),
    Proxy {
        proxy: ProxyLedger,
        hello: LedgerHello,
        why: String,
    },
    Neither(String),
}

async fn database(config: &ProxyConfig) -> Result<PersistHandle, String> {
    match crate::config::build_persist_handle(config, SchemaPolicy::RequireCurrent).await {
        Ok(Some(handle)) => Ok(handle.background()),
        Ok(None) => Err("persistence is disabled".to_string()),
        Err(e) => Err(format!("{e:#}")),
    }
}

pub async fn reach(config: &ProxyConfig) -> Reach {
    let why = match database(config).await {
        Ok(handle) => return Reach::Database(handle),
        Err(why) => why,
    };
    match ProxyLedger::connect(&config.inspect_listen_addr).await {
        Ok((proxy, hello)) => Reach::Proxy { proxy, hello, why },
        Err(proxy_why) => Reach::Neither(format!("{why}; {proxy_why}")),
    }
}

pub async fn fold_settings(config: &mut ProxyConfig, reach: &Reach) {
    match reach {
        Reach::Database(handle) => {
            crate::config::db_fold::fold_db_settings(Some(handle), config).await
        }
        Reach::Proxy { hello, .. } => {
            crate::config::apply_settings_overrides(config, &hello.settings);
        }
        Reach::Neither(_) => {}
    }
}

fn through_proxy(proxy: ProxyLedger, hello: &LedgerHello, why: &str) -> Ledger {
    eprintln!(
        "mike spawn: the database is unreachable from this process ({why}); launches are \
         recorded through the proxy at {}",
        proxy.addr()
    );
    Ledger::through_proxy(proxy).with_mail_briefs(hello.launch_brief)
}

async fn store(config: &ProxyConfig, reach: Reach) -> Ledger {
    match reach {
        Reach::Database(handle) => Ledger::new(Some(handle)).read_mail_briefs().await,
        Reach::Proxy { proxy, hello, why } => through_proxy(proxy, &hello, &why),
        Reach::Neither(why) => match ProxyLedger::connect(&config.inspect_listen_addr).await {
            Ok((proxy, hello)) => through_proxy(proxy, &hello, &why),
            Err(_) => {
                eprintln!(
                    "mike spawn: ledger unavailable ({why}); launches are queued locally until \
                     the database or the proxy answers"
                );
                Ledger::none()
            }
        },
    }
}

pub async fn ledger(config: &ProxyConfig, reach: Reach) -> (Ledger, Replay) {
    let ledger = store(config, reach)
        .await
        .with_queue(LocalQueue::default_dir());
    let replay = queue_replay::replay(&ledger).await;
    (ledger, replay)
}
