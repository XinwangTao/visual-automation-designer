/**
 * Pointer spotlight, adapted from Inspira UI's CardSpotlight (MIT).
 *
 * Returns handlers that write the pointer position into CSS variables; the
 * visual layer (`lg-spotlight`) paints a soft radial highlight at that point.
 */
import { useCallback } from 'react';

export interface SpotlightHandlers {
  onPointerMove: (event: React.MouseEvent<HTMLElement>) => void;
  onPointerLeave: (event: React.MouseEvent<HTMLElement>) => void;
  onPointerEnter: (event: React.MouseEvent<HTMLElement>) => void;
}

export function useSpotlight(): SpotlightHandlers {
  const onPointerMove = useCallback((event: React.MouseEvent<HTMLElement>) => {
    const target = event.currentTarget;
    const rect = target.getBoundingClientRect();
    target.style.setProperty('--spot-x', `${event.clientX - rect.left}px`);
    target.style.setProperty('--spot-y', `${event.clientY - rect.top}px`);
  }, []);

  const reset = useCallback((event: React.MouseEvent<HTMLElement>) => {
    event.currentTarget.style.setProperty('--spot-x', '-9999px');
    event.currentTarget.style.setProperty('--spot-y', '-9999px');
  }, []);

  return { onPointerMove, onPointerLeave: reset, onPointerEnter: onPointerMove };
}
