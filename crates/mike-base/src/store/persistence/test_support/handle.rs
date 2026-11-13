use std::sync::Arc;

use tokio::sync::OnceCell;
use uuid::Uuid;

use super::provisioning::{
    create_database_from_template, drop_database, ensure_run_db, guard_against_capture_instance,
    guard_against_dev_db, prepare_template_db, run_db_name,
};
use super::url::replace_db_name;

static BOOTSTRAP: OnceCell<Option<Arc<TestDbContext>>> = OnceCell::const_new();

static RUN_DB_READY: OnceCell<String> = OnceCell::const_new();

#[derive(Debug)]
struct TestDbContext {
    admin_url: String,
}

#[derive(Debug, Clone)]
pub struct TestDb {
    ctx: Arc<TestDbContext>,
}

impl TestDb {
    pub async fn bootstrap() -> anyhow::Result<Option<Self>> {
        let maybe_ctx = BOOTSTRAP
            .get_or_try_init(|| async {
                let admin_url = super::requirements::admin_url()?;
                guard_against_dev_db(&admin_url)?;
                guard_against_capture_instance(&admin_url).await?;
                prepare_template_db(&admin_url).await?;
                anyhow::Ok(Some(Arc::new(TestDbContext { admin_url })))
            })
            .await?;
        Ok(maybe_ctx.as_ref().map(|ctx| Self { ctx: ctx.clone() }))
    }

    pub async fn acquire_shared(&self) -> anyhow::Result<TestDbHandle> {
        let admin_url = self.ctx.admin_url.clone();
        let db_name = RUN_DB_READY
            .get_or_try_init(|| async {
                let name = run_db_name();
                ensure_run_db(&admin_url, &name).await?;
                anyhow::Ok(name)
            })
            .await?
            .clone();
        let database_url = replace_db_name(&self.ctx.admin_url, &db_name)?;
        Ok(TestDbHandle {
            database_url,
            admin_url: self.ctx.admin_url.clone(),
            db_name,
            cleaned_up: false,
            shared: true,
        })
    }

    pub async fn acquire(&self) -> anyhow::Result<TestDbHandle> {
        self.acquire_shared().await
    }

    pub async fn acquire_isolated(&self) -> anyhow::Result<TestDbHandle> {
        let db_name = format!("claude_proxy_test_{}", Uuid::new_v4().simple());
        create_database_from_template(&self.ctx.admin_url, &db_name).await?;
        let database_url = replace_db_name(&self.ctx.admin_url, &db_name)?;
        Ok(TestDbHandle {
            database_url,
            admin_url: self.ctx.admin_url.clone(),
            db_name,
            cleaned_up: false,
            shared: false,
        })
    }
}

pub async fn acquire_ephemeral_db() -> anyhow::Result<Option<TestDbHandle>> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    test_db.acquire_isolated().await.map(Some)
}

pub struct TestDbHandle {
    pub database_url: String,
    admin_url: String,
    db_name: String,
    cleaned_up: bool,
    shared: bool,
}

impl TestDbHandle {
    pub async fn cleanup(mut self) -> anyhow::Result<()> {
        if self.cleaned_up || self.shared {
            return Ok(());
        }
        drop_database(&self.admin_url, &self.db_name).await?;
        self.cleaned_up = true;
        Ok(())
    }
}

impl std::fmt::Debug for TestDbHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestDbHandle")
            .field("db_name", &self.db_name)
            .field("cleaned_up", &self.cleaned_up)
            .finish()
    }
}

impl Drop for TestDbHandle {
    fn drop(&mut self) {
        if self.cleaned_up || self.shared {
            return;
        }
        let admin_url = self.admin_url.clone();
        let db_name = self.db_name.clone();

        let join = std::thread::spawn(move || run_cleanup_drop(admin_url, db_name));
        let _ = join.join();
    }
}

fn build_cleanup_runtime(db_name: &str) -> Option<tokio::runtime::Runtime> {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => Some(rt),
        Err(e) => {
            tracing::warn!(
                db = %db_name,
                error = %e,
                "TestDbHandle::Drop: failed to build cleanup runtime"
            );
            None
        }
    }
}

fn run_cleanup_drop(admin_url: String, db_name: String) {
    let Some(rt) = build_cleanup_runtime(&db_name) else {
        return;
    };
    if let Err(e) = rt.block_on(drop_database(&admin_url, &db_name)) {
        tracing::warn!(
            db = %db_name,
            error = %e,
            "TestDbHandle::Drop cleanup failed; ephemeral DB may be orphaned"
        );
    }
}
