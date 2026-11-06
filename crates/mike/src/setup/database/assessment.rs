use anyhow::{Result, bail, ensure};
use sqlx::PgPool;

pub(super) const SUPPORTED_SERVER_MAJORS: [i32; 1] = [19];

pub(super) struct Facts {
    pub server_major: i32,
    pub superuser: bool,
    recovery: bool,
    owns_database: bool,
    creates_schema: bool,
    creates_in_public: bool,
    public_is_current: bool,
    pub has_ledger: bool,
    has_tables: bool,
    owns_tables: bool,
}

pub(super) async fn read(pool: &PgPool) -> Result<Facts, sqlx::Error> {
    sqlx::query_as!(
        Facts,
        r#"SELECT (current_setting('server_version_num')::int / 10000) AS "server_major!",
                r.rolsuper AS "superuser!",
                pg_is_in_recovery() AS "recovery!",
                pg_has_role(d.datdba, 'USAGE') AS "owns_database!",
                has_database_privilege(current_database(), 'CREATE') AS "creates_schema!",
                (has_schema_privilege('public', 'CREATE') AND has_schema_privilege('public', 'USAGE')) AS "creates_in_public!",
                (current_schema() = 'public') AS "public_is_current!",
                (to_regclass('public._sqlx_migrations') IS NOT NULL) AS "has_ledger!",
                (count(c.oid) > 0) AS "has_tables!",
                COALESCE(bool_and(pg_has_role(c.relowner, 'USAGE')), true) AS "owns_tables!"
         FROM pg_database d
         JOIN pg_roles r ON r.rolname = current_user
         LEFT JOIN pg_namespace n ON n.nspname NOT IN ('pg_catalog', 'information_schema')
                                  AND n.nspname NOT LIKE 'pg_toast%'
                                  AND n.nspname NOT LIKE 'pg_temp%'
         LEFT JOIN pg_class c ON c.relnamespace = n.oid AND c.relkind IN ('r', 'p', 'S', 'v', 'm')
         LEFT JOIN pg_depend e ON e.classid = 'pg_class'::regclass AND e.objid = c.oid AND e.deptype = 'e'
         WHERE d.datname = current_database() AND e.objid IS NULL
         GROUP BY r.rolsuper, d.datdba"#,
    )
    .fetch_one(pool)
    .await
}

impl Facts {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            !self.recovery,
            "the selected database is a read-only recovery replica; select its writable primary"
        );
        ensure!(
            SUPPORTED_SERVER_MAJORS.contains(&self.server_major),
            "mike requires PostgreSQL 19; upgrade other server majors before connecting"
        );
        ensure!(
            self.public_is_current,
            "the database role must use public as its current schema; remove a conflicting search_path setting"
        );
        ensure!(
            self.superuser
                || (self.owns_database
                    && self.creates_schema
                    && self.creates_in_public
                    && self.owns_tables),
            "the migration role needs database and application-object ownership, CREATE on the database and CREATE/USAGE on public; ask the database administrator to grant these permissions"
        );
        ensure!(
            self.has_ledger || !self.has_tables,
            "existing application tables have no public._sqlx_migrations ledger; restore the original ledger or choose a fresh database; setup will not invent migration history"
        );
        Ok(())
    }
}

struct Extension {
    name: String,
    installed: bool,
    available: bool,
    trusted: bool,
    public_schema: bool,
}

pub(super) async fn extensions(
    pool: &PgPool,
    superuser: bool,
    require_installed: bool,
) -> Result<()> {
    let extensions = sqlx::query_as!(
        Extension,
        r#"SELECT required.name AS "name!", (e.oid IS NOT NULL) AS "installed!",
                (a.name IS NOT NULL) AS "available!", COALESCE(v.trusted, false) AS "trusted!",
                COALESCE(n.nspname = 'public', true) AS "public_schema!"
         FROM (VALUES ('vector'), ('pgcrypto'), ('pg_trgm')) AS required(name)
         LEFT JOIN pg_available_extensions a ON a.name = required.name
         LEFT JOIN pg_available_extension_versions v ON v.name = a.name AND v.version = a.default_version
         LEFT JOIN pg_extension e ON e.extname = required.name
         LEFT JOIN pg_namespace n ON n.oid = e.extnamespace
         ORDER BY required.name"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| anyhow::anyhow!("required extension assessment failed; check access to PostgreSQL extension catalogs"))?;
    for extension in extensions {
        validate_extension(extension, superuser, require_installed)?;
    }
    Ok(())
}

fn validate_extension(
    extension: Extension,
    superuser: bool,
    require_installed: bool,
) -> Result<()> {
    ensure!(
        !require_installed || extension.installed,
        "current migration ledger requires PostgreSQL extension {} to be installed; ask the database administrator to restore the extension before retrying",
        extension.name
    );
    ensure!(
        extension.available,
        "required PostgreSQL extension {} is unavailable; ask the database administrator to install its server package",
        extension.name
    );
    ensure!(
        extension.public_schema,
        "required PostgreSQL extension {} must be in the public schema",
        extension.name
    );
    if !extension.installed && !extension.trusted && !superuser {
        bail!(
            "required PostgreSQL extension {} is not enabled; ask the database administrator to CREATE EXTENSION in public before retrying",
            extension.name
        );
    }
    Ok(())
}

pub(super) async fn require_successful_ledger(pool: &PgPool) -> Result<()> {
    let failed = sqlx::query_scalar!(
        r#"SELECT count(*) AS "failed!" FROM public._sqlx_migrations WHERE NOT success"#
    )
        .fetch_one(pool)
        .await
        .map_err(|_| anyhow::anyhow!("migration ledger is unreadable or malformed; restore its original structure and permissions"))?;
    ensure!(
        failed == 0,
        "the database has a failed migration; resolve it with the matching binary before setup; no history was changed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            server_major: 19,
            superuser: true,
            recovery: false,
            owns_database: true,
            creates_schema: true,
            creates_in_public: true,
            public_is_current: true,
            has_ledger: false,
            has_tables: false,
            owns_tables: true,
        }
    }

    #[test]
    fn database_facts_require_supported_major_and_original_ledger() -> Result<()> {
        facts().validate()?;
        let mut value = facts();
        value.server_major = 16;
        assert!(value.validate().is_err());
        value.server_major = 17;
        assert!(value.validate().is_err());
        value.server_major = 20;
        assert!(value.validate().is_err());
        value.server_major = 18;
        assert!(value.validate().is_err());
        value.server_major = 19;
        value.validate()?;
        value.server_major = 19;
        value.has_tables = true;
        assert!(
            value
                .validate()
                .err()
                .ok_or_else(|| anyhow::anyhow!("tables without ledger"))?
                .to_string()
                .contains("ledger")
        );
        Ok(())
    }

    #[test]
    fn database_facts_reject_missing_permissions() {
        let mut value = facts();
        value.superuser = false;
        value.owns_tables = false;
        assert!(value.validate().is_err());
    }

    #[test]
    fn extension_validation_requires_package_and_creation_permission() -> Result<()> {
        let ext = |available, installed| Extension {
            name: "vector".into(),
            available,
            installed,
            trusted: false,
            public_schema: true,
        };
        assert!(validate_extension(ext(false, false), true, false).is_err());
        assert!(validate_extension(ext(true, false), false, false).is_err());
        validate_extension(ext(true, true), false, false)?;
        validate_extension(ext(true, false), true, false)
    }

    #[test]
    fn current_ledger_requires_installed_extensions_for_every_role() -> Result<()> {
        for superuser in [false, true] {
            for trusted in [false, true] {
                let extension = |installed| Extension {
                    name: "pg_trgm".into(),
                    available: true,
                    installed,
                    trusted,
                    public_schema: true,
                };
                assert!(validate_extension(extension(false), superuser, true).is_err());
                validate_extension(extension(true), superuser, true)?;
                if superuser || trusted {
                    validate_extension(extension(false), superuser, false)?;
                }
            }
        }
        Ok(())
    }
}
