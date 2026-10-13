// Small text helpers shared by the call screen and the preview.
export const words = t => t.split(/\s+/).filter(Boolean);

// What the status line says, matching page.html's copy.
export function statusText(v) {
  switch (v.phase) {
    case 'incoming': return 'Incoming call';
    case 'connecting': return 'Connecting…';
    case 'talking': return 'Claude is talking…';
    case 'listening': return v.muted ? 'Mic muted, type a reply' : 'Listening… take your time';
    case 'hearing': return 'Hearing you…';
    case 'thinking': return v.sub === 'transcribing' ? 'Got it, transcribing…' : 'Claude is thinking…';
    case 'hold': return 'On hold';
    case 'ended': return 'Call ended';
  }
  return '';
}

export const clock = ms => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};

