import ws from 'k6/ws';
import { check, sleep } from 'k6';

// k6 Load Test: 10,000 Concurrent WebSockets (Testing Strategy §5)
// Target: p95 move-ack <= 100ms server-side; zero dropped frames.

export const options = {
  stages: [
    { duration: '30s', target: 500 },    // Warm-up
    { duration: '1m', target: 2000 },    // Ramp to 2k CCU
    { duration: '2m', target: 5000 },    // Ramp to 5k CCU
    { duration: '2m', target: 10000 },   // Peak 10k CCU load
    { duration: '1m', target: 0 },       // Cool-down
  ],
  thresholds: {
    'ws_connecting': ['p(95)<200'],
    'ws_msgs_received': ['count>10000'],
  },
};

const BASE_URL = __ENV.TARGET_URL || 'ws://127.0.0.1:5173/ws';

export default function () {
  const url = `${BASE_URL}?ticket=mock_k6_ticket_${__VU}`;
  const params = { tags: { my_tag: 'ws_concurrency' } };

  const res = ws.connect(url, params, function (socket) {
    socket.on('open', function () {
      // Send heartbeat ping every 15s
      socket.setInterval(function () {
        socket.send(JSON.stringify({ type: 'ping', ts: Date.now() }));
      }, 15000);

      // Simulate match move every 20s
      socket.setInterval(function () {
        socket.send(
          JSON.stringify({
            type: 'game.move',
            ch: `g:game_${__VU % 100}`,
            data: { uci: 'e2e4', ply: 1 },
          })
        );
      }, 20000);
    });

    socket.on('message', function (data) {
      const msg = JSON.parse(data);
      check(msg, {
        'message received without error': (m) => !m.error,
      });
    });

    socket.on('close', function () {
      // Connection closed cleanly
    });

    socket.on('error', function (e) {
      console.error('WS Error:', e.error());
    });

    // Keep session active for 45s
    sleep(45);
    socket.close();
  });

  check(res, { 'status is 101': (r) => r && r.status === 101 });
}
