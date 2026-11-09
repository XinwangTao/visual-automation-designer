/**
 * BlockGlyph — small inline SVG glyphs for block types.
 *
 * Emoji render differently across Windows/macOS and look inconsistent at
 * 15–16px. These paths use `currentColor`, so the tinted icon tiles control the
 * colour and every theme stays coherent.
 */

export type BlockGlyphType =
  | 'click'
  | 'wait_image'
  | 'wait_time'
  | 'input_text'
  | 'screenshot_assert'
  | 'text_extract'
  | 'loop'
  | 'loop_infinite'
  | 'condition'
  | 'text_check'
  | string;

const GLYPHS: Record<string, string> = {
  click: 'M4 2.2 12.8 8l-3.9 1.1L7 13z',
  wait_image: 'M2.2 3.2h11.6v9.6H2.2zM2.2 10.2l3.2-3.2 3 3 2-2 3.4 3.4',
  wait_time: 'M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12zM8 4.6V8l2.4 1.5',
  input_text: 'M2.2 4.5h11.6v7H2.2zM4.4 6.7h.1M6.6 6.7h.1M8.8 6.7h.1M11 6.7h.1M4.4 9.3h7.2',
  screenshot_assert: 'M2.2 5.2h3l1-1.6h3.6l1 1.6h3v7.2H2.2zM8 7.2a2.3 2.3 0 1 0 0 4.6 2.3 2.3 0 0 0 0-4.6z',
  text_extract: 'M1.8 8S4.1 4.2 8 4.2 14.2 8 14.2 8 11.9 11.8 8 11.8 1.8 8 1.8 8zM8 6.5a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3z',
  loop: 'M3.4 6.4A4.8 4.8 0 0 1 8 3.2h4.6M10.8 1.6 12.8 3.2l-2 1.6M12.6 9.6A4.8 4.8 0 0 1 8 12.8H3.4M5.2 11.2 3.2 12.8l2 1.6',
  loop_infinite: 'M4.2 6.2c-1.6 0-2.8 1-2.8 1.8s1.2 1.8 2.8 1.8c2.4 0 5.2-3.6 7.6-3.6 1.6 0 2.8 1 2.8 1.8s-1.2 1.8-2.8 1.8c-2.4 0-5.2-3.6-7.6-3.6z',
  condition: 'M8 1.8 14.2 8 8 14.2 1.8 8zM6.6 6.2A1.6 1.6 0 0 1 8 5.2c.9 0 1.6.7 1.6 1.5 0 1.2-1.6 1.3-1.6 2.4M8 11v.2',
  text_check: 'M3 2.4h7.4L13 5v8.6H3zM5 8.2l1.8 1.8L10.8 6',
};

export interface BlockGlyphProps {
  type: BlockGlyphType;
  size?: number;
  className?: string;
}

export function BlockGlyph({ type, size = 15, className }: BlockGlyphProps) {
  const path = GLYPHS[type] ?? GLYPHS.click;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      className={className}
      aria-hidden="true"
      focusable="false"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d={path} />
    </svg>
  );
}

export default BlockGlyph;
