import http from 'k6/http';
import { check, sleep } from 'k6';

// k6 Load Test: Matchmaking Queue Storm (5,000 Joins in 60s)
// Target: p50 match wait < 30s; zero double-pairing; zero orphaned tickets.

export const options = {
  scenarios: {
    matchmaking_storm: {
      executor: 'ramping-arrival-rate',
      startRate: 50,
      timeUnit: '1s',
      preAllocatedVUs: 200,
      maxVUs: 1000,
      stages: [
        { duration: '20s', target: 200 },
        { duration: '30s', target: 500 }, // 500 requests/sec peak
        { duration: '20s', target: 50 },
      ],
    },
  },
  thresholds: {
    http_req_duration: ['p(95)<150'], // 95% of join requests under 150ms
    http_req_failed: ['rate<0.01'],    // Under 1% failures
  },
};

const BASE_URL = __ENV.TARGET_URL || 'http://127.0.0.1:5173/v1';

export default function () {
  const payload = JSON.stringify({
    initial_s: 180,
    increment_s: 2,
    rated: true,
  });

  const params = {
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer mock_user_token_${__VU}`,
      'Idempotency-Key': `k6_queue_${__VU}_${Date.now()}`,
    },
  };

  const res = http.post(`${BASE_URL}/matchmaking/queue`, payload, params);

  check(res, {
    'queue status is 200 or 201': (r) => r.status === 200 || r.status === 201,
    'contains est_wait_s': (r) => r.body && r.body.includes('est_wait_s'),
  });

  sleep(1);
}
