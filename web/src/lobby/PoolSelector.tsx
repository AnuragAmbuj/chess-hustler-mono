import { PoolType, TimeControlConfig } from '../types';
import { useAuth } from '../auth/useAuth';

export interface PoolConfig {
  id: PoolType;
  title: string;
  icon: string;
  subtitle: string;
  timeControls: TimeControlConfig[];
}

export const POOLS: PoolConfig[] = [
  {
    id: 'bullet',
    title: 'Bullet',
    icon: '⚡',
    subtitle: '< 3 min per player',
    timeControls: [
      { initial_s: 60, increment_s: 0, pool: 'bullet', label: '1+0' },
      { initial_s: 60, increment_s: 2, pool: 'bullet', label: '1+2' },
      { initial_s: 120, increment_s: 1, pool: 'bullet', label: '2+1' },
    ],
  },
  {
    id: 'blitz',
    title: 'Blitz',
    icon: '🔥',
    subtitle: '3 to 5 min per player',
    timeControls: [
      { initial_s: 180, increment_s: 0, pool: 'blitz', label: '3+0' },
      { initial_s: 180, increment_s: 2, pool: 'blitz', label: '3+2' },
      { initial_s: 300, increment_s: 0, pool: 'blitz', label: '5+0' },
    ],
  },
  {
    id: 'rapid',
    title: 'Rapid',
    icon: '⏱️',
    subtitle: '10 to 15 min per player',
    timeControls: [
      { initial_s: 600, increment_s: 0, pool: 'rapid', label: '10+0' },
      { initial_s: 900, increment_s: 10, pool: 'rapid', label: '15+10' },
    ],
  },
  {
    id: 'classical',
    title: 'Classical',
    icon: '🏛️',
    subtitle: '30+ min per player',
    timeControls: [
      { initial_s: 1800, increment_s: 0, pool: 'classical', label: '30+0' },
    ],
  },
];

interface PoolSelectorProps {
  selectedTc: TimeControlConfig;
  onSelectTc: (tc: TimeControlConfig) => void;
  disabled?: boolean;
}

export function PoolSelector({
  selectedTc,
  onSelectTc,
  disabled = false,
}: PoolSelectorProps) {
  const { user } = useAuth();

  return (
    <div className="pool-selector-section">
      <div className="section-header">
        <h2 className="section-title">Play Chess</h2>
        <span className="section-desc">Choose a time control to find a live opponent or practice</span>
      </div>

      <div className="pool-cards-grid">
        {POOLS.map((pool) => {
          const userRating = user ? user.ratings[pool.id] : 1200;
          const isPoolActive = selectedTc.pool === pool.id;

          return (
            <div
              key={pool.id}
              className={`pool-panel ${isPoolActive ? 'pool-panel-active' : ''}`}
            >
              <div className="pool-panel-header">
                <div className="pool-panel-info">
                  <span className="pool-panel-icon">{pool.icon}</span>
                  <div>
                    <h3 className="pool-panel-title">{pool.title}</h3>
                    <span className="pool-panel-sub">{pool.subtitle}</span>
                  </div>
                </div>
                <div className="pool-rating-chip tabular-nums" title={`${pool.title} rating`}>
                  {userRating}
                </div>
              </div>

              <div className="tc-pills-row">
                {pool.timeControls.map((tc) => {
                  const isSelected =
                    selectedTc.initial_s === tc.initial_s &&
                    selectedTc.increment_s === tc.increment_s;

                  return (
                    <button
                      key={tc.label}
                      type="button"
                      className={`tc-pill ${isSelected ? 'tc-pill-active' : ''}`}
                      onClick={() => onSelectTc(tc)}
                      disabled={disabled}
                    >
                      <span className="tc-pill-label tabular-nums">{tc.label}</span>
                    </button>
                  );
                })}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
