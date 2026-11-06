SELECT required.name, e.oid IS NOT NULL AS installed,
                a.name IS NOT NULL AS available, COALESCE(v.trusted, false) AS trusted,
                COALESCE(n.nspname = 'public', true) AS public_schema
         FROM (VALUES ('vector'), ('pgcrypto'), ('pg_trgm')) AS required(name)
         LEFT JOIN pg_available_extensions a ON a.name = required.name
         LEFT JOIN pg_available_extension_versions v ON v.name = a.name AND v.version = a.default_version
         LEFT JOIN pg_extension e ON e.extname = required.name
         LEFT JOIN pg_namespace n ON n.oid = e.extnamespace
         ORDER BY required.name;
