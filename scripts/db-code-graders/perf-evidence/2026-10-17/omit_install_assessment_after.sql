SELECT current_setting('server_version_num')::int / 10000 AS server_major,
                r.rolsuper AS superuser,
                pg_is_in_recovery() AS recovery,
                pg_has_role(d.datdba, 'USAGE') AS owns_database,
                has_database_privilege(current_database(), 'CREATE') AS creates_schema,
                (has_schema_privilege('public', 'CREATE') AND has_schema_privilege('public', 'USAGE')) AS creates_in_public,
                current_schema() = 'public' AS public_is_current,
                to_regclass('public._sqlx_migrations') IS NOT NULL AS has_ledger,
                count(c.oid) > 0 AS has_tables,
                COALESCE(bool_and(pg_has_role(c.relowner, 'USAGE')), true) AS owns_tables
         FROM pg_database d
         JOIN pg_roles r ON r.rolname = current_user
         LEFT JOIN pg_namespace n ON n.nspname NOT IN ('pg_catalog', 'information_schema')
                                  AND n.nspname NOT LIKE 'pg_toast%'
                                  AND n.nspname NOT LIKE 'pg_temp%'
         LEFT JOIN pg_class c ON c.relnamespace = n.oid AND c.relkind IN ('r', 'p', 'S', 'v', 'm')
         LEFT JOIN pg_depend e ON e.classid = 'pg_class'::regclass AND e.objid = c.oid AND e.deptype = 'e'
         WHERE d.datname = current_database() AND e.objid IS NULL
         GROUP BY r.rolsuper, d.datdba;
