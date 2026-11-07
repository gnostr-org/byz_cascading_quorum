import init, { run_example, set_log_callback } from './pkg/byz_time.js';

self.addEventListener('message', async ({ data }) => {
  if (data.type !== 'run') return;

  try {
    await init();
    set_log_callback((line) => self.postMessage({ type: 'log', line }));
    run_example(data.example);
    self.postMessage({ type: 'complete' });
  } catch (error) {
    self.postMessage({ type: 'error', message: String(error) });
  }
});
