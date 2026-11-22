-- 0007_seed_missions_and_achievements.sql — seed default mission and achievement definitions

INSERT INTO mission_defs (id, kind, title_key, goal_type, goal_params, reward_gold, active) VALUES
  ('daily_games_1',     'daily',  'mission.daily_games_1',     'games_played', '{"target": 1}',  250,  true),
  ('daily_games_3',     'daily',  'mission.daily_games_3',     'games_played', '{"target": 3}',  500,  true),
  ('daily_win_1',       'daily',  'mission.daily_win_1',       'wins',         '{"target": 1}',  400,  true),
  ('daily_checkmate_1', 'daily',  'mission.daily_checkmate_1', 'checkmates',   '{"target": 1}',  750,  true),
  ('weekly_games_15',   'weekly', 'mission.weekly_games_15',   'games_played', '{"target": 15}', 2500, true),
  ('weekly_wins_7',     'weekly', 'mission.weekly_wins_7',     'wins',         '{"target": 7}',  3000, true)
ON CONFLICT (id) DO NOTHING;

INSERT INTO achievement_defs (id, title_key, goal_type, goal_params, reward_gold, active) VALUES
  ('achieve_first_win',   'achievement.first_win',   'wins',         '{"target": 1}',    500,  true),
  ('achieve_games_10',    'achievement.games_10',    'games_played', '{"target": 10}',   1000, true),
  ('achieve_games_50',    'achievement.games_50',    'games_played', '{"target": 50}',   5000, true),
  ('achieve_checkmate_5', 'achievement.checkmate_5', 'checkmates',   '{"target": 5}',    2500, true),
  ('achieve_streak_7',    'achievement.streak_7',    'streak_days',  '{"target": 7}',    3000, true),
  ('achieve_rating_1500', 'achievement.rating_1500', 'rating_peak',  '{"target": 1500}', 5000, true)
ON CONFLICT (id) DO NOTHING;
