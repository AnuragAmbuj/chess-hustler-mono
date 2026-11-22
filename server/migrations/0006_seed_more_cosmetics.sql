-- 0006_seed_more_cosmetics.sql — expand cosmetic catalog with avatars, frames, pieces, and boards

INSERT INTO cosmetics (id, category, name_key, price_gold, rarity, preview) VALUES
  ('avatar_street_hustler', 'avatar', 'cosmetic.avatar_street_hustler.name', 5000,  'common', '{"icon": "♟️", "badge": "Hustler"}'),
  ('avatar_grandmaster',    'avatar', 'cosmetic.avatar_grandmaster.name',    15000, 'rare',   '{"icon": "👑", "badge": "Grandmaster"}'),
  ('avatar_cyber_king',     'avatar', 'cosmetic.avatar_cyber_king.name',     35000, 'epic',   '{"icon": "⚡", "badge": "Cyber King"}'),
  ('frame_bronze',          'frame',  'cosmetic.frame_bronze.name',          4000,  'common', '{"border_color": "#cd7f32"}'),
  ('frame_gold_trim',       'frame',  'cosmetic.frame_gold_trim.name',       12000, 'rare',   '{"border_color": "#d97706"}'),
  ('frame_neon_edge',       'frame',  'cosmetic.frame_neon_edge.name',       25000, 'epic',   '{"border_color": "#38bdf8"}'),
  ('pieces_modern_wood',    'pieces', 'cosmetic.pieces_modern_wood.name',    8000,  'rare',   '{"style": "wood"}'),
  ('board_dark_slate',      'board',  'cosmetic.board_dark_slate.name',      6000,  'common', '{"theme": "slate"}')
ON CONFLICT (id) DO NOTHING;
