// The preview's own pieces; everything else is shared with the real call screen in call-ui.
export * from '../../call-ui/src/ui.jsx';

// The phone: Galaxy S25 Ultra at 412 x 892 CSS px, a punch-hole camera, the status bar
// and the gesture bar. Insets: 36 px top, 24 px bottom.
export function StatusBar() {
  return (
    <div className="statusbar" aria-hidden="true">
      <span>9:41</span>
      <span className="sb-icons">
        <svg width="16" height="12" viewBox="0 0 16 12"><path d="M1 11h2V8H1zM5 11h2V6H5zM9 11h2V3.5H9zM13 11h2V1h-2z" fill="currentColor" /></svg>
        <svg width="16" height="12" viewBox="0 0 16 12"><path d="M8 11.2 1 4.4a10 10 0 0 1 14 0z" fill="currentColor" /></svg>
        <svg width="24" height="12" viewBox="0 0 24 12"><rect x=".5" y=".5" width="20" height="11" rx="3" fill="none" stroke="currentColor" opacity=".5" /><rect x="2" y="2" width="14" height="8" rx="1.6" fill="currentColor" /><path d="M22 4v4" stroke="currentColor" strokeLinecap="round" opacity=".5" /></svg>
      </span>
    </div>
  );
}
