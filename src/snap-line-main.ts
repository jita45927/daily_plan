import { invoke } from '@tauri-apps/api/core';

document.body.addEventListener('mouseenter', () => {
  invoke('expand_from_snap_line').then(() => {
  }).catch(() => {
  });
});
