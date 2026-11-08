# perf-query-sha.sh — shared SQL fingerprint for the perf-evidence binding (temet-ai/mike-private#192).
#
# Sourced by scripts/db-perf-check (emits the `Perf-Query-SHA:` trailer for the
# measured AFTER query) and by scripts/db-code-graders/g6_perf_evidence.sh
# (recomputes the fingerprint over the after-snapshot STAGED in the commit and
# requires equality). Keep both sides on THIS implementation — drift between
# them turns every honest commit into a false block.
#
# Normalization makes the fingerprint invariant to exactly the differences the
# measuring workflow legitimately introduces between the shipped query and the
# measured snapshot, and nothing else:
#   - case, whitespace/indentation, `--` comments, semicolons
#   - $N placeholders vs the substituted literal values (string and numeric
#     literals and $N all collapse to `?`, as does a cast glued to such a value)

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/sha256.sh"

# stdin: SQL text → stdout: normalized single-line form.
perf_sql_normalize() {
  sed -e 's/--.*$//' \
    | tr '[:upper:]' '[:lower:]' \
    | sed -E -e "s/'[^']*'/?/g" -e 's/\$[0-9]+/?/g' -e 's/[0-9]+(\.[0-9]+)?/?/g' \
    | tr -d '[:space:];' \
    | sed -E -e 's/\?::[a-z_?]+(\[\])?/?/g' -e 's/\?+/?/g'
}

# stdin: SQL text → stdout: 16-hex fingerprint of the normalized form.
# Returns 1 (empty output) when no sha tool exists — callers treat that as
# "fingerprint unavailable", never as a block (fail-open on missing tooling).
perf_query_sha() {
  local hash=""
  hash="$(perf_sql_normalize | sha256)" || return 1
  [[ -n "$hash" ]] || return 1
  printf '%s\n' "${hash:0:16}"
}

# SQL keywords/builtins that appear in ANY query and therefore prove nothing
# about which query was measured. Used by the g6 identifier-overlap binding.
PERF_SQL_STOPWORDS='select|insert|update|delete|merge|into|values|set|from|where|and|or|not|null|is|in|exists|between|like|ilike|similar|escape|as|on|using|join|left|right|inner|outer|full|cross|lateral|group|order|by|having|limit|offset|distinct|union|intersect|except|all|any|some|case|when|then|else|end|asc|desc|nulls|first|last|with|recursive|returning|conflict|do|nothing|excluded|cascade|restrict|create|alter|drop|table|index|view|materialized|sequence|schema|function|trigger|constraint|primary|foreign|key|references|unique|check|default|begin|commit|rollback|explain|analyze|buffers|verbose|costs|cast|coalesce|nullif|greatest|least|count|sum|avg|min|max|array|array_agg|string_agg|jsonb_agg|json_agg|unnest|row_number|rank|dense_rank|over|partition|window|filter|within|text|int2|int4|int8|integer|bigint|smallint|numeric|decimal|real|float4|float8|double|precision|boolean|bool|char|varchar|character|varying|date|time|timestamp|timestamptz|interval|json|jsonb|uuid|bytea|serial|bigserial|now|current_timestamp|current_date|localtimestamp|epoch|extract|date_trunc|date_part|to_char|to_timestamp|lower|upper|length|substring|position|strpos|split_part|replace|concat|concat_ws|trim|btrim|ltrim|rtrim|regexp_replace|regexp_matches|abs|ceil|ceiling|floor|round|random|md5|gen_random_uuid|nextval|setval|currval|for|share|nowait|skip|locked|temp|temporary|if|only|add|column|type|rename|owner|grant|revoke|btree|gin|gist|brin|hash|spgist|include|concurrently|collate|true|false|unknown'

# stdin: SQL text → stdout: sorted unique identifier tokens (length ≥ 3,
# stopwords and quoted-string contents removed). The g6 gate requires a
# majority of these to appear in the staged code the commit actually ships.
perf_sql_tokens() {
  { sed -e 's/--.*$//' \
      | tr '[:upper:]' '[:lower:]' \
      | sed -E "s/'[^']*'/ /g" \
      | grep -oE '[a-z_][a-z0-9_]*' \
      | grep -vwE "(${PERF_SQL_STOPWORDS})" \
      | awk 'length($0) >= 3' \
      | LC_ALL=C sort -u; } 2>/dev/null || true
}
