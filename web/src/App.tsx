import { useEffect, useState } from 'react';

/**
 * Phase 0 shell: proves the client↔server contract path end-to-end.
 * Board/lobby/economy UIs land in Phase 1 (docs/08-roadmap/roadmap.md).
 * Config values render from the server contract — never hardcoded client-side.
 */

interface MetaConfig {
  server_time_unix_ms: number;
  economy: { welcome_gold: number };
  ads: { gold_per_ad: number; max_per_day: number; cooldown_s: number };
  time_controls: { initial_s: number; increment_s: number; pool: string }[];
}

export function App() {
  const [config, setConfig] = useState<MetaConfig | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch('/v1/meta/config')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json() as Promise<MetaConfig>;
      })
      .then(setConfig)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <main className="shell">
      <h1>
        ♞ Chess Hustle <span className="badge">Phase 0</span>
      </h1>
      <p className="sub">
        Foundation build — the board, matchmaking and economy arrive in Phase 1.
      </p>

      {error && <p className="error">API error: {error} (is hustle-server running on :8080?)</p>}

      {config && (
        <section className="card">
          <h2>Server contract check ✓</h2>
          <ul>
            <li>
              Welcome grant: <strong>{config.economy.welcome_gold.toLocaleString()} Gold</strong>
              <em> (fictional, non-monetary, non-purchasable — the Gold Economy)</em>
            </li>
            <li>
              Rewarded ads: <strong>{config.ads.gold_per_ad.toLocaleString()} Gold</strong> per ad,
              max <strong>{config.ads.max_per_day}/day</strong>
            </li>
            <li>
              Time-control presets: <strong>{config.time_controls.length}</strong> across{' '}
              {new Set(config.time_controls.map((t) => t.pool)).size} pools
            </li>
          </ul>
        </section>
      )}
    </main>
  );
}
