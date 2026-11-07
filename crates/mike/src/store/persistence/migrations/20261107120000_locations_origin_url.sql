-- The git remote of a checkout, recorded by the machine that holds it. Paths
-- differ between machines; the remote is what says two checkouts are one repository.
SET LOCAL lock_timeout = '5s';
ALTER TABLE projects.locations ADD COLUMN origin_url TEXT;
CREATE INDEX locations_origin_url ON projects.locations (origin_url)
    WHERE origin_url IS NOT NULL;
