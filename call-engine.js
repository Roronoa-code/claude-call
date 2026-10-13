// Claude Call engine: the live call logic, shared by the call pages (page.html now, the new design next).
// Plain script; served by server.mjs as /call-engine.js?t=<token>. No UI here: the page renders the events.
//
//   const call = CallEngine({ token, on: { status(text) { ... }, partial(u) { ... }, ... } });
//   call.answer()           start the call (from the Answer tap: mobile audio needs a tap)
//   call.finish()           "Done talking": send what the user said now
//   call.sendTyped(text)    typed reply instead of speech
//   call.setMuted(on)       mute / unmute the mic (returns the new state)
//   call.setSpeaker(on)     earpiece / loudspeaker inside the Claude Call app (returns the new state)
//   call.hasSpeaker, call.muted, call.speaker
//
// Events (all optional):
//   state(s)           every /state from the server: { seq, say, end, hold, holdLabel, mode, ... }
//   status(text)       short status line ("Listening…", "Claude is talking…")
//   busy(on)           Claude is talking or thinking (e.g. pulse the avatar)
//   claude(line)       a line of Claude's starts sounding: { id, text }   (shown only once its voice starts)
//   playing(line)      ~10x a second while it plays: { id, text, ms, dur, shown }; `shown` is the text revealed
//                      in step with the real audio position; `dur` is 0 until the whole line has arrived
//   spoken(line)       it finished: { id, text, cut }  (cut = the user talked over it)
//   partial(u)         live caption while the user talks: { id, text }
//   heard(u)           the user's final words: { id, text, typed }  ('' = only noise: drop that bubble)
//   interrupted(x)     the user cut Claude off: { reason: 'words' | 'loud' }
//   level(x)           the user's mic level, 0..1, while they talk
//   canFinish(on)      whether "Done talking" makes sense now
//   muted(on)
//   ended()            the call is over
(function () {
  'use strict';
  const FRAME = 2048;          // mic frames (~43 ms at 48 kHz)
  const PREROLL_MS = 500;      // audio kept from just before speech starts
  const SILENCE_MS = 1800;     // end of turn after this much quiet...
  const EARLY_MS = 700;        // ...or this much, when the live transcript reads as a finished sentence
  const PARTIAL_MS = 900;      // live caption refresh while talking
  const PAUSE_MS = 250;        // a pause this long asks for a fresh caption at once (for the early end of turn)
  const MAX_MS = 90_000;
  // Talking over Claude. While Claude's voice plays, the phone's echo cancelling turns the user down a lot, so the
  // bar is relative to the measured echo level, and quieter speech is confirmed by real words from Whisper.
  const BARGE_MIN = 0.006, BARGE_ECHO = 3, LOUD_MIN = 0.03, LOUD_ECHO = 8;
  const MS_PER_CHAR = 65;      // the cloned voice's pace, for captions until a line's length is known
  const CONT = /\b(and|but|so|or|because|cause|the|a|an|to|of|um+|uh+|er|like|if|then|with|for|that|which|my|your|our|is|are|was|were|just|maybe|also|well|what|when|where|how|why|i|i'm|we|you|it's|in|on|at|about)$/i;
  const BACKCHANNEL = /^(m+-?h?m+|uh-?huh|mhm|hmm+)[.!]?$/i;
  const SILENT_WAV = 'data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQAAAAA=';
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const wordsOf = t => (t || '').toLowerCase().match(/[a-z0-9']+/g) || [];
  // Does the live transcript read as a finished turn? ("Yes." / "Push it now." but not "I think that we should and")
  const looksComplete = text => {
    const s = (text || '').trim();
    return /[.!?]["')\]]?$/.test(s) && !/(\.\.\.|…)$/.test(s) && !CONT.test(s.replace(/[.!?"')\]]+$/, ''));
  };
  // Real words from the user: not Claude's own voice leaking back, not a "mm-hmm".
  const realWords = (text, line) => {
    if (!text || BACKCHANNEL.test(text.trim())) return false;
    const own = new Set(wordsOf(line));
    return wordsOf(text).some(w => !own.has(w));
  };

  window.CallEngine = function ({ token, on = {} }) {
    const api = p => `${p}?t=${token}`;
    const emit = (name, x) => { try { on[name]?.(x); } catch (e) { console.error(e); } };
    const app = window.CallMeApp; // inside the Claude Call app: native earpiece audio
    const player = new Audio();
    let ended = false, muted = false, debug = false;
    let ctx = null, rate = 48000, micStream = null, micNode = null;
    let noiseFloor = 0.01, echo = 0.004, preroll = [];
    let mode = 'idle';      // idle | listen (waiting for speech) | talk (user speaking) | claude (Claude's line)
    let utt = null, barge = null, cutIn = false, quietSince = 0, startedAt = 0;
    let hot = [], loudRun = 0;
    let line = null;        // Claude's line now: { id, text, startAt, pos, posAt, posFn, dur, cut, audioId }
    let stopLine = null;    // silences it at once

    // ---- debug log (server writes it to timing.log when debug is on) ----
    const dq = [];
    let st = { n: 0, sum: 0, max: 0, low: 0, high: 0 };
    const dbg = msg => { if (debug) dq.push(`${(performance.now() / 1000).toFixed(2)} ${msg}`); };
    setInterval(() => {
      if (!debug || ended) return;
      if (micStream) {
        dq.push(`${(performance.now() / 1000).toFixed(2)} ${mode} frames=${st.n} rms avg=${(st.sum / (st.n || 1)).toFixed(4)} max=${st.max.toFixed(4)}` +
          ` echo=${echo.toFixed(4)} floor=${noiseFloor.toFixed(4)}` + (mode === 'claude' ? ` thr=${st.low.toFixed(4)}/${st.high.toFixed(4)}` : '') +
          (app?.device ? ` out=${app.device()}` : ''));
      }
      st = { n: 0, sum: 0, max: 0, low: 0, high: 0 };
      if (dq.length) fetch(api('debug'), { method: 'POST', body: dq.splice(0).join('\n') }).catch(() => {});
    }, 1000);

    // ---- the user's voice: mic on the phone, live transcripts and the final one from Whisper on the PC ----
    const audioCtx = () => { if (!ctx) ctx = new AudioContext(); ctx.resume(); return ctx; };
    async function startMic() {
      micStream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } });
      const c = audioCtx(); rate = c.sampleRate;
      micNode = c.createScriptProcessor(FRAME, 1, 1);
      c.createMediaStreamSource(micStream).connect(micNode); micNode.connect(c.destination);
      micNode.onaudioprocess = e => onFrame(new Float32Array(e.inputBuffer.getChannelData(0)), performance.now());
    }
    const newUtt = frames => {
      const u = { id: Math.random().toString(36).slice(2, 10), unsent: [], raw: 0, sentRaw: 0, sent16: 0, speechEndRaw: 0,
        partial: null, inflight: null, again: false, lastAsk: 0, asked: false, closed: false };
      for (const f of frames) addFrame(u, f);
      return u;
    };
    const addFrame = (u, f) => { u.unsent.push(f); u.raw += f.length; };
    const to16 = raw => Math.round(raw * 16000 / rate);
    // The audio not sent yet, as 16 kHz 16-bit PCM.
    function take(u) {
      const len = u.unsent.reduce((n, c) => n + c.length, 0), all = new Float32Array(len);
      let o = 0; for (const c of u.unsent) { all.set(c, o); o += c.length; }
      const ratio = rate / 16000, out = new Int16Array(Math.floor(len / ratio));
      for (let i = 0; i < out.length; i++) {
        const a = Math.floor(i * ratio), b = Math.min(len, Math.floor((i + 1) * ratio));
        let s = 0; for (let j = a; j < b; j++) s += all[j];
        out[i] = Math.max(-1, Math.min(1, s / Math.max(1, b - a))) * 32767;
      }
      u.unsent = []; u.sentRaw = u.raw;
      const off = u.sent16; u.sent16 += out.length;
      return { off, body: out.buffer };
    }
    // Sends what's new and gets the transcript so far (one request at a time per utterance).
    async function ask(u) {
      if (u.closed) return;
      if (u.inflight) { u.again = true; return; }
      u.lastAsk = performance.now();
      const { off, body } = take(u), covers = u.sentRaw, t0 = performance.now();
      u.inflight = fetch(`${api('listen')}&u=${u.id}&off=${off}&partial=1`, { method: 'POST', body }).then(r => r.json()).catch(() => null);
      const r = await u.inflight; u.inflight = null;
      if (r?.text != null && !u.closed) {
        u.partial = { text: r.text, covers };
        if (u === barge) {
          dbg(`barge words "${r.text}" (${Math.round(performance.now() - t0)}ms)`);
          if (realWords(r.text, line?.text)) interrupt('words', performance.now());
        } else if (u === utt && r.text) emit('partial', { id: u.id, text: r.text });
      }
      if (u.again && !u.closed) { u.again = false; ask(u); }
    }

    function onFrame(f, now) {
      let sum = 0; for (let i = 0; i < f.length; i++) sum += f[i] * f[i];
      const rms = Math.sqrt(sum / f.length);
      st.n++; st.sum += rms; st.max = Math.max(st.max, rms);
      preroll.push(f); while (preroll.length * FRAME / rate * 1000 > PREROLL_MS) preroll.shift();
      if (muted || ended) return;
      if (mode === 'claude') return bargeFrame(f, rms, now);
      if (mode === 'idle') { noiseFloor = noiseFloor * 0.95 + rms * 0.05; return; }
      const speech = rms > Math.max(0.012, noiseFloor * 3);
      if (mode === 'listen') {
        if (speech) startTalking(newUtt(preroll), false, now); else noiseFloor = noiseFloor * 0.95 + rms * 0.05;
        return;
      }
      addFrame(utt, f); // talking
      emit('level', Math.min(1, rms * 12));
      if (speech) { quietSince = now; utt.speechEndRaw = utt.raw; utt.asked = false; }
      const quiet = now - quietSince;
      if (quiet < PAUSE_MS ? now - utt.lastAsk > PARTIAL_MS : !utt.asked) { utt.asked = quiet >= PAUSE_MS; ask(utt); }
      const p = utt.partial;
      const early = quiet > EARLY_MS && p && p.covers >= utt.speechEndRaw && looksComplete(p.text);
      if (early || quiet > SILENCE_MS || now - startedAt > MAX_MS) finish(early ? 'early' : 'silence');
    }
    function bargeFrame(f, rms, now) {
      const low = Math.max(BARGE_MIN, echo * BARGE_ECHO, noiseFloor * 2), high = Math.max(LOUD_MIN, echo * LOUD_ECHO);
      st.low = low; st.high = high;
      // Learns how loud Claude's own voice comes back: falls fast, rises slowly (so the user's voice doesn't count).
      if (!barge) echo = rms < echo ? echo * 0.9 + rms * 0.1 : echo * 0.99 + rms * 0.01;
      // Clearly loud speech stops Claude at once (not in a line's first moments, while the echo level is still being learnt).
      loudRun = rms > high ? loudRun + 1 : 0;
      if (loudRun >= 4 && line?.startAt && now - line.startAt > 800) return interrupt('loud', now);
      hot.push(rms > low); if (hot.length > 7) hot.shift();
      if (!barge) {
        if (hot.filter(Boolean).length >= 5) { barge = newUtt(preroll); barge.born = barge.lastHot = now; dbg(`barge? rms=${rms.toFixed(4)} thr=${low.toFixed(4)}`); }
        return;
      }
      addFrame(barge, f);
      if (rms > low) barge.lastHot = now;
      if (now - barge.lastHot > 700) { dbg('barge dropped'); barge.closed = true; barge = null; return; }
      if (now - barge.born > 200 && now - barge.lastAsk > 500) ask(barge);
    }
    function interrupt(reason, now) {
      const t0 = performance.now();
      stopLine?.();
      const took = performance.now() - t0;
      if (line) fetch(`${api('hush')}&seq=${line.id}`, { method: 'POST' }).catch(() => {}); // stop making the rest of the line
      const u = barge || newUtt(preroll);
      barge = null; hot = []; loudRun = 0;
      if (line) line.cut = true;
      startTalking(u, true, now);
      emit('interrupted', { reason });
      dbg(`interrupt (${reason}) "${u.partial?.text || ''}" audio stopped in ${took.toFixed(0)}ms`);
    }
    function startTalking(u, interrupted, now) {
      utt = u; mode = 'talk'; cutIn = interrupted; startedAt = quietSince = now; u.speechEndRaw = u.raw;
      emit('status', 'Hearing you…'); emit('busy', false); emit('canFinish', true);
      if (u.partial?.text) emit('partial', { id: u.id, text: u.partial.text });
    }
    function listen() {
      if (ended) return;
      if (muted || !micStream) { mode = 'idle'; emit('status', micStream ? 'Mic muted, type a reply' : 'Type your reply'); emit('busy', false); return; }
      ctx.resume(); mode = 'listen';
      emit('status', 'Listening… take your time'); emit('busy', false);
    }
    function dropUtterance() {
      if (utt) utt.closed = true;
      utt = null; mode = 'idle'; emit('canFinish', false); emit('level', 0);
    }
    async function finish(why = 'button') {
      if (mode !== 'talk') return;
      const u = utt, quiet = Math.round(performance.now() - quietSince), wasCut = cutIn;
      dropUtterance(); cutIn = false;
      emit('status', 'Got it…'); emit('busy', true);
      dbg(`turn end (${why}) after ${quiet}ms quiet, caption "${u.partial?.text || ''}"`);
      const { off, body } = take(u);
      try {
        const r = await (await fetch(`${api('speech')}&u=${u.id}&off=${off}&end=${to16(u.speechEndRaw)}&quiet=${quiet}${wasCut ? '&interrupted=1' : ''}`,
          { method: 'POST', body })).json();
        emit('heard', { id: u.id, text: r.text });
        if (r.text) emit('status', 'Claude is thinking…'); else if (mode === 'idle' && !line) listen();
      } catch { emit('status', 'Could not send audio, type instead'); }
    }

    // ---- Claude's voice ----
    const appWaits = {};
    window.__played = (id, ok) => { appWaits[id]?.(ok); delete appWaits[id]; };
    window.__audioStart = id => { if (line?.audioId === id) started(); };
    window.__audioDur = (id, ms) => { if (line?.audioId === id) line.dur = ms; };
    window.__audioPos = (id, ms) => { if (line?.audioId === id) { line.pos = ms; line.posAt = performance.now(); } };
    // Native playback; resolves true / false (failed) when the line ends.
    const appPlay = (method, url, id, words) => new Promise(res => {
      appWaits[id] = res;
      setTimeout(() => window.__played(id, true), 20000 + words * 700); // never hang the call
      app[method](url, id);
    });
    function started() {
      if (!line || line.startAt) return;
      line.startAt = performance.now();
      emit('claude', { id: line.id, text: line.text });
    }
    // Captions in step with the voice: reveal by how far the audio has played.
    function tick() {
      if (!line?.startAt) return;
      const ms = line.posFn ? line.posFn() : line.posAt ? line.pos + performance.now() - line.posAt : performance.now() - line.startAt;
      const total = line.dur || Math.max(line.text.length * MS_PER_CHAR, ms / 0.95);
      const end = line.text.indexOf(' ', Math.ceil(Math.min(1, ms / total) * line.text.length)); // to the end of the word
      const shown = end < 0 ? line.text : line.text.slice(0, end);
      emit('playing', { id: line.id, text: line.text, ms: Math.round(ms), dur: line.dur, shown });
    }
    async function webStream(url) {
      // Browser (not the app): play the voice with Web Audio as its pieces arrive (see /stream in server.mjs).
      const c = audioCtx(), ac = new AbortController(), srcs = [];
      let at = 0, t0 = 0, bytes = 0, carry = null, cut = false, wake = null;
      stopLine = () => { cut = true; ac.abort(); srcs.forEach(s => { try { s.stop(); } catch {} }); wake?.(); };
      try {
        for (let from = 0; ;) {
          const r = await fetch(`${url}&from=${from}`, { signal: ac.signal });
          if (!r.ok) { if (from) break; return false; } // 404 later on = the line was replaced
          const sr = Number(r.headers.get('x-sample-rate')) || 24000, last = r.headers.get('x-done') === '1';
          let b = new Uint8Array(await r.arrayBuffer());
          from += b.length;
          if (carry) { const m = new Uint8Array(b.length + 1); m[0] = carry; m.set(b, 1); b = m; carry = null; }
          if (b.length & 1) { carry = b[b.length - 1]; b = b.subarray(0, b.length - 1); }
          const n = b.length >> 1;
          if (!n) { if (last) break; continue; }
          const dv = new DataView(b.buffer, b.byteOffset, b.length), f = new Float32Array(n);
          for (let i = 0; i < n; i++) f[i] = dv.getInt16(2 * i, true) / 32768;
          const buf = c.createBuffer(1, n, sr); buf.copyToChannel(f, 0);
          const src = c.createBufferSource(); src.buffer = buf; src.connect(c.destination);
          if (!at) { at = t0 = c.currentTime + 0.05; line.posFn = () => Math.max(0, (c.currentTime - t0) * 1000); started(); }
          at = Math.max(at, c.currentTime); src.start(at); at += buf.duration; srcs.push(src); bytes += b.length;
          line.dur = last ? bytes / 2 / sr * 1000 : 0;
          if (last) break;
        }
        await new Promise(r => { wake = r; setTimeout(r, Math.max(0, (at - c.currentTime) * 1000) + 50); });
        return bytes > 0 || cut;
      } catch { return cut; }
    }
    function filePlay(url, words) {
      // A whole file (fallback voices) in the browser.
      return new Promise(res => {
        const backup = setTimeout(done, 5000 + words * 600);
        function done() { clearTimeout(backup); player.onended = player.onerror = player.onplaying = null; res(true); }
        stopLine = () => { player.pause(); done(); };
        player.onended = done;
        player.onplaying = started;
        player.onerror = () => { clearTimeout(backup); res(false); };
        player.src = url;
        player.play().catch(() => { clearTimeout(backup); res(false); });
      });
    }
    function speakBrowser(text) {
      // Last resort: the phone's own voice.
      return new Promise(res => {
        const u = new SpeechSynthesisUtterance(text);
        u.lang = 'en-GB';
        const backup = setTimeout(res, 2000 + wordsOf(text).length * 450);
        u.onstart = started;
        u.onend = u.onerror = () => { clearTimeout(backup); res(); };
        stopLine = () => { speechSynthesis.cancel(); res(); };
        speechSynthesis.cancel(); speechSynthesis.speak(u);
      });
    }
    const getState = async () => (await fetch(`${api('state')}&v=-1`, { cache: 'no-store' })).json();
    async function playLine(s) {
      line = { id: s.seq, text: s.say, startAt: 0, pos: 0, posAt: 0, posFn: null, dur: 0, cut: false, audioId: s.seq * 1000 };
      const me = line, timer = setInterval(tick, 100);
      mode = 'claude'; hot = []; loudRun = 0;
      emit('status', 'Claude is talking…'); emit('busy', true);
      const streamUrl = new URL(`${api('stream')}&seq=${s.seq}`, location.href).href;
      try {
        // Ask for the stream straight away: the server holds it until the first sound is ready (409 = use parts).
        const appStop = () => { app.stop(); window.__played(me.audioId, true); };
        let ok;
        if (app?.stream) { stopLine = appStop; ok = await appPlay('stream', streamUrl, me.audioId, wordsOf(s.say).length); }
        else ok = await webStream(streamUrl);
        if (!ok && !me.cut) {
          let st = await getState();
          if (st.seq === s.seq && st.mode === 'parts') {
            for (let i = 0; i < st.parts.length && !me.cut; i++) {
              while (st.seq === s.seq && st.ready <= i && !me.cut) { await sleep(150); st = await getState(); }
              if (st.seq !== s.seq || me.cut) break;
              const url = new URL(`${api('audio')}&seq=${s.seq}&part=${i}`, location.href).href, words = wordsOf(st.parts[i]).length;
              me.audioId = s.seq * 1000 + 1 + i;
              if (app) stopLine = appStop;
              ok = app ? await appPlay('play', url, me.audioId, words) : await filePlay(url, words);
              if (!ok && !me.cut) await speakBrowser(st.parts[i]);
            }
          } else if (st.seq === s.seq && app?.stream && st.mode === 'stream' && (ok = await webStream(streamUrl))) dbg('native voice failed; played in the page');
          else if (st.seq === s.seq) await speakBrowser(s.say);
        }
      } finally {
        clearInterval(timer);
        started(); // a line that never sounded still gets shown
        if (!me.cut) emit('playing', { id: me.id, text: me.text, ms: me.dur, dur: me.dur, shown: me.text });
        emit('spoken', { id: me.id, text: me.text, cut: me.cut });
        line = null; stopLine = null;
        if (mode === 'claude') mode = 'idle';
      }
    }

    async function loop() {
      let v = -1, lastSeq = null;
      while (!ended) {
        let s;
        try { s = await (await fetch(`${api('state')}&v=${v}`, { cache: 'no-store' })).json(); }
        catch { emit('status', 'Reconnecting…'); await sleep(1000); continue; }
        v = s.v; debug = s.debug; emit('state', s);
        if (!s.say) { emit('status', 'Connecting…'); continue; }
        if (s.seq === lastSeq) continue;
        lastSeq = s.seq;
        if (mode === 'talk') finish('new line'); // keep what they were saying (it reaches Claude's next turn)
        await playLine(s);
        if (s.end) return end();
        if (mode === 'talk') continue; // they cut in: already listening
        if (barge) { const b = barge; barge = null; startTalking(b, false, performance.now()); } // started talking as it ended
        else listen();
      }
    }
    function end() {
      ended = true; mode = 'idle';
      emit('status', 'Call ended'); emit('busy', false); emit('canFinish', false);
      micStream?.getTracks().forEach(tr => tr.stop()); ctx?.close(); // release the mic
      emit('ended');
      if (app) setTimeout(() => app.ended(), 1200);
    }

    return {
      async answer() {
        player.src = SILENT_WAV; player.play().catch(() => {}); // mobile audio must first start from a tap
        if (!app?.stream) audioCtx();
        app?.begin?.(); // call audio (earpiece) from now on
        emit('status', 'Connected');
        try { await startMic(); } catch { muted = true; emit('muted', true); }
        fetch(api('answer'), { method: 'POST' });
        loop();
      },
      finish: () => finish('button'),
      async sendTyped(text) {
        text = (text || '').trim(); if (!text) return;
        if (mode === 'talk' || mode === 'listen') dropUtterance();
        emit('heard', { id: 'typed-' + Date.now(), text, typed: true });
        emit('status', 'Claude is thinking…'); emit('busy', true);
        await fetch(api('reply'), { method: 'POST', body: text });
      },
      async setMuted(on) {
        if (!on && !micStream) { try { await startMic(); } catch { emit('status', 'Mic blocked, type a reply'); return muted; } }
        muted = on; emit('muted', muted);
        if (muted) {
          if (mode === 'talk' || mode === 'listen') dropUtterance();
          if (barge) { barge.closed = true; barge = null; }
          emit('status', 'Mic muted, type a reply');
        } else if (mode === 'idle' && !line) listen();
        return muted;
      },
      setSpeaker(on) { app?.setSpeaker(on); return !!on; },
      get hasSpeaker() { return !!app; },
      get speaker() { return !!app?.isSpeaker(); },
      get muted() { return muted; },
    };
  };
})();
