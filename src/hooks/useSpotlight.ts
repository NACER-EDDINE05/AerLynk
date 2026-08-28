import { useEffect, useRef } from 'react';

/**
 * Tracks the pointer across a container and feeds `--mx` / `--my` custom
 * properties to whichever `.card` is hovered, driving the spotlight-border
 * glow. One rAF-throttled listener for the whole app — no per-card listeners.
 */
export function useSpotlight<T extends HTMLElement>() {
  const ref = useRef<T>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let raf = 0;
    const onMove = (e: PointerEvent) => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        const card = (e.target as HTMLElement | null)?.closest?.('.card');
        if (!(card instanceof HTMLElement)) return;
        const rect = card.getBoundingClientRect();
        card.style.setProperty('--mx', `${e.clientX - rect.left}px`);
        card.style.setProperty('--my', `${e.clientY - rect.top}px`);
      });
    };
    el.addEventListener('pointermove', onMove);
    return () => {
      el.removeEventListener('pointermove', onMove);
      if (raf) cancelAnimationFrame(raf);
    };
  }, []);

  return ref;
}