// Ronron's cat features, shared by every place that draws him (the engine, the
// launch greeting and the file drop): ginger fur, ears, tail, tabby stripes,
// nose, mouth and whiskers.
//
// Every function draws around the body's centre (0, 0), the body being about
// `rx` wide and `ry` tall from the centre. Ears and tail go *behind* the body
// (call them before filling it); stripes and the muzzle go on top of it.

/** Ginger fur, light on top, warmer at the bottom. */
export const FUR_TOP = "#F9C48A";
export const FUR_BOTTOM = "#E8904A";
const FUR_TOP_RGB = [0.976, 0.769, 0.541] as const;
const FUR_BOTTOM_RGB = [0.91, 0.565, 0.29] as const;
export const FUR_TOP_RGB01 = FUR_TOP_RGB;
export const FUR_BOTTOM_RGB01 = FUR_BOTTOM_RGB;
const STRIPE = "rgba(178,88,30,0.55)";
const EAR_INNER = "rgba(247,160,176,0.95)";
const NOSE = "#E2677A";

export type Fill = string | CanvasGradient;

/** The fur gradient for a body of half-size rx × ry. */
export function furGradient(x: CanvasRenderingContext2D, rx: number, ry: number): CanvasGradient {
  const g = x.createLinearGradient(rx * 0.6, -ry * 0.95, -rx * 0.7, ry * 0.95);
  g.addColorStop(0, FUR_TOP);
  g.addColorStop(1, FUR_BOTTOM);
  return g;
}

export interface EarOptions {
  /** Head turn, radians (0 = facing you). */
  yaw?: number;
  /** 0…1: how much of the ears shows (they fold away when he becomes a box). */
  presence?: number;
  /** Seconds, for the occasional twitch. Omit for still ears. */
  time?: number;
  /** Pink inside the ears. Off for tiny sizes. */
  inner?: boolean;
  /** A state colour washed over the ears, as over the body. */
  tint?: { color: string; amount: number };
}

/** Two pointy ears on top of the head. Draw before the body. */
export function drawEars(x: CanvasRenderingContext2D, rx: number, ry: number, fill: Fill, o: EarOptions = {}) {
  const presence = o.presence ?? 1;
  if (presence <= 0.02) return;
  const yaw = o.yaw ?? 0;
  for (const sd of [-1, 1]) {
    const a = sd * 0.62 + yaw;
    const facing = Math.cos(a);
    if (facing <= 0.05) continue;
    const baseX = Math.sin(a) * rx * 0.8;
    const baseY = -ry * 0.62;
    const bw = rx * 0.34 * Math.max(0.45, facing);
    const eh = ry * 0.72 * presence;
    // Now and then one ear flicks: a quick small turn outwards.
    let twitch = 0;
    if (o.time !== undefined) {
      const phase = (o.time + (sd > 0 ? 2.3 : 0)) % 6.5;
      if (phase < 0.22) twitch = Math.sin((phase / 0.22) * Math.PI) * 0.22 * sd;
    }
    x.save();
    x.translate(baseX, baseY);
    x.rotate(sd * 0.2 + twitch);
    earPath(x, bw, eh);
    x.fillStyle = fill;
    x.fill();
    if (o.tint && o.tint.amount > 0.01) {
      x.globalAlpha = o.tint.amount;
      x.fillStyle = o.tint.color;
      x.fill();
      x.globalAlpha = 1;
    }
    if (o.inner !== false) {
      earPath(x, bw * 0.55, eh * 0.72, ry * 0.02);
      x.fillStyle = EAR_INNER;
      x.fill();
    }
    x.restore();
  }
}

/** A rounded-tip triangle standing on (0, 0), `h` tall, `w` half-wide at the base. */
function earPath(x: CanvasRenderingContext2D, w: number, h: number, lift = 0) {
  const tipR = Math.min(w, h) * 0.22;
  const by = h * 0.35 - lift; // the base sinks into the head
  x.beginPath();
  x.moveTo(-w, by);
  x.lineTo(-tipR * 0.9, -h + tipR);
  x.quadraticCurveTo(0, -h - tipR * 0.4, tipR * 0.9, -h + tipR);
  x.lineTo(w, by);
  x.closePath();
}

/** A fluffy tail curling up behind his right side. Draw before the body. */
export function drawTail(
  x: CanvasRenderingContext2D, rx: number, ry: number, R: number,
  color: string, time: number, presence = 1,
) {
  if (presence <= 0.02) return;
  const sway = Math.sin(time * 1.6) * 0.18 + Math.sin(time * 0.7) * 0.06;
  x.save();
  x.globalAlpha *= Math.min(1, presence);
  x.translate(rx * 0.72, ry * 0.5);
  x.rotate(sway * 0.6);
  x.strokeStyle = color;
  x.lineCap = "round";
  x.lineWidth = R * 0.22;
  // Out to the side, then up, the tip curling away from the body.
  x.beginPath();
  x.moveTo(0, 0);
  x.bezierCurveTo(rx * 0.42, ry * 0.12, rx * 0.5, -ry * 0.45, rx * 0.36 + sway * rx * 0.12, -ry * 0.92);
  x.stroke();
  // Darker tip.
  x.strokeStyle = STRIPE;
  x.lineWidth = R * 0.23;
  x.beginPath();
  x.moveTo(rx * 0.44, -ry * 0.6);
  x.quadraticCurveTo(rx * 0.42, -ry * 0.8, rx * 0.36 + sway * rx * 0.12, -ry * 0.92);
  x.stroke();
  x.restore();
}

/** Three tabby stripes on the forehead. Draw clipped to the body. */
export function drawStripes(x: CanvasRenderingContext2D, rx: number, ry: number, yaw = 0, alpha = 1) {
  if (alpha <= 0.02) return;
  const off = Math.sin(yaw) * rx * 0.8;
  x.save();
  x.globalAlpha *= alpha;
  x.fillStyle = STRIPE;
  for (const k of [-1, 0, 1]) {
    const cx = off + k * rx * 0.17;
    const len = k === 0 ? ry * 0.42 : ry * 0.32;
    const w = rx * (k === 0 ? 0.055 : 0.045);
    x.beginPath();
    x.moveTo(cx - w, -ry * 1.05);
    x.quadraticCurveTo(cx - w * 0.6, -ry + len * 0.6, cx, -ry + len);
    x.quadraticCurveTo(cx + w * 0.6, -ry + len * 0.6, cx + w, -ry * 1.05);
    x.closePath();
    x.fill();
  }
  x.restore();
}

export interface MuzzleOptions {
  yaw?: number;
  /** Where the nose sits, as a fraction of ry below the centre. */
  noseY?: number;
  whiskers?: boolean;
  alpha?: number;
}

/** Pink nose, a little "ω" mouth and whiskers. Draw on top of the body, unclipped. */
export function drawMuzzle(x: CanvasRenderingContext2D, rx: number, ry: number, R: number, ink: string, o: MuzzleOptions = {}) {
  const alpha = o.alpha ?? 1;
  if (alpha <= 0.02) return;
  const yaw = o.yaw ?? 0;
  const mx = Math.sin(yaw) * rx * 0.8;
  const ny = ry * (o.noseY ?? 0.36);
  const s = R * 0.07;
  x.save();
  x.globalAlpha *= alpha;
  // Nose: a soft rounded triangle.
  x.fillStyle = NOSE;
  x.beginPath();
  x.moveTo(mx - s, ny - s * 0.55);
  x.quadraticCurveTo(mx, ny - s * 0.85, mx + s, ny - s * 0.55);
  x.quadraticCurveTo(mx + s * 0.2, ny + s * 0.6, mx, ny + s * 0.65);
  x.quadraticCurveTo(mx - s * 0.2, ny + s * 0.6, mx - s, ny - s * 0.55);
  x.fill();
  // Mouth: ω.
  x.strokeStyle = ink;
  x.lineWidth = Math.max(0.8, R * 0.028);
  x.lineCap = "round";
  x.beginPath();
  x.moveTo(mx, ny + s * 0.6);
  x.quadraticCurveTo(mx, ny + s * 1.6, mx - s * 1.1, ny + s * 1.35);
  x.moveTo(mx, ny + s * 0.6);
  x.quadraticCurveTo(mx, ny + s * 1.6, mx + s * 1.1, ny + s * 1.35);
  x.stroke();
  if (o.whiskers !== false) {
    x.globalAlpha *= 0.55;
    x.lineWidth = Math.max(0.6, R * 0.018);
    for (const sd of [-1, 1]) {
      // The whiskers on the side turning away get shorter.
      const reach = rx * (0.62 + 0.3 * (1 + sd * Math.sin(yaw)) / 2);
      for (const k of [-1, 0, 1]) {
        const y0 = ny + s * 0.35 + k * s * 0.55;
        x.beginPath();
        x.moveTo(mx + sd * s * 1.8, y0);
        x.quadraticCurveTo(mx + sd * reach * 0.6, y0 + k * s * 0.3 - s * 0.15, mx + sd * reach, y0 + k * s * 1.1);
        x.stroke();
      }
    }
  }
  x.restore();
}
