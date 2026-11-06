--
-- eval.class_map rows for the two-axis turn verdicts.
--
-- JC08's daily accuracy rollup scores a production verdict class against a label value
-- through eval.class_map (production_axis, production_class, label_axis, label_value,
-- replied_counts_as_negative). JC08 seeded only the legacy mappings (legacy `corrected`
-- against reaction `corrected`, legacy `abandoned` against a frustration session end),
-- because until JC09 no verdict carried a reaction or a delivery. memory.turn_verdicts
-- (JC09) does, so the accuracy_turn_verdicts family can now score each of the five
-- reaction classes and six delivery classes against the label of the same name, for the
-- live and shadow arms and per bundle. No class counts replied turns as negatives: these
-- axes are labelled per turn, and a turn without a label on the axis is simply unscored.
--
-- Idempotent: ON CONFLICT on the table's primary key (production_axis, production_class).

INSERT INTO eval.class_map
    (production_axis, production_class, label_axis, label_value, replied_counts_as_negative)
VALUES
    ('reaction', 'corrected',      'reaction', 'corrected',      false),
    ('reaction', 'redirected',     'reaction', 'redirected',     false),
    ('reaction', 'accepted',       'reaction', 'accepted',       false),
    ('reaction', 'no_response',    'reaction', 'no_response',    false),
    ('reaction', 'unclear',        'reaction', 'unclear',        false),
    ('delivery', 'shipped',        'delivery', 'shipped',        false),
    ('delivery', 'delivered',      'delivery', 'delivered',      false),
    ('delivery', 'partial',        'delivery', 'partial',        false),
    ('delivery', 'none',           'delivery', 'none',           false),
    ('delivery', 'question',       'delivery', 'question',       false),
    ('delivery', 'not_applicable', 'delivery', 'not_applicable', false)
ON CONFLICT (production_axis, production_class) DO NOTHING;
