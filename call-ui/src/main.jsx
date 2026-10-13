import { createRoot } from 'react-dom/client';
import CallD from './callD.jsx';
import { CallView } from './call.js';
import { MascotKind, Personality } from './mascot.jsx';
import './call.css';

// Liquid's open spring (damping 0.75, decay 5.4, about 2.8% overshoot), sampled into a CSS linear() curve.
(function springCurve() {
  const D = 0.75, DECAY = 5.4, W = DECAY / D, WD = W * Math.sqrt(1 - D * D);
  const pts = [];
  for (let i = 0; i <= 40; i++) {
    const t = i / 40;
    pts.push((1 - Math.exp(-D * W * t) * (Math.cos(WD * t) + ((D * W) / WD) * Math.sin(WD * t))).toFixed(4));
  }
  pts[40] = '1';
  document.documentElement.style.setProperty('--spring-liquid', `linear(${pts.join(', ')})`);
})();

const call = new CallView();
window.__call = call; // for checks
createRoot(document.getElementById('root')).render(
  <MascotKind.Provider value="clawd">
    <Personality.Provider value="mixed">
      <CallD sim={call} />
    </Personality.Provider>
  </MascotKind.Provider>,
);
