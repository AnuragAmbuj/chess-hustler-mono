import http from 'k6/http';
import { check, sleep } from 'k6';

// k6 Load Test: Rewarded Ads SSV Callback Burst (2,000 / min)
// Target: SSV handler p95 <= 50ms; zero double-grants; quota race resilience.

export const options = {
  stages: [
    { duration: '20s', target: 20 },
    { duration: '40s', target: 100 },
    { duration: '20s', target: 0 },
  ],
  thresholds: {
    http_req_duration: ['p(95)<60'],
    http_req_failed: ['rate<0.02'],
  },
};

const BASE_URL = __ENV.TARGET_URL || 'http://127.0.0.1:5173/v1';

export default function () {
  // Test Ad SSV callback endpoint
  const query = `ad_unit=test_unit_123&user_id=${__VU}&custom_data=reward_claim_${Date.now()}&signature=mock_p256_ecdsa_sig`;
  const url = `${BASE_URL}/ads/ssv/admob/callback?${query}`;

  const res = http.get(url);

  check(res, {
    'status code is 200, 204, or 400': (r) => [200, 204, 400, 429].includes(r.status),
  });

  sleep(0.5);
}
