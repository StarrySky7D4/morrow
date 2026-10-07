import { Point, Rect } from '@kit.TestKit';

export class ImagePointerPlan {
  operation: string = '';
  fingers: number = 0;
  steps: number = 9;
  speed: number = 600;
  points: Point[][] = [];
}

export function validImageBounds(bounds: Rect): boolean {
  return Number.isFinite(bounds.left) && Number.isFinite(bounds.top) && Number.isFinite(bounds.right) &&
    Number.isFinite(bounds.bottom) && bounds.left >= 0 && bounds.top >= 0 &&
    bounds.right - bounds.left >= 120 && bounds.bottom - bounds.top >= 120;
}

export function sameImageBounds(actual: Rect, expected: Rect): boolean {
  return validImageBounds(actual) && validImageBounds(expected) &&
    Math.abs(actual.left - expected.left) <= 1 && Math.abs(actual.top - expected.top) <= 1 &&
    Math.abs(actual.right - expected.right) <= 1 && Math.abs(actual.bottom - expected.bottom) <= 1;
}

// Fractions describe a requested trace within the freshly observed canvas, not
// a guessed device coordinate. The runner first requires exact expected bounds.
export function imagePointerPlan(operation: string, bounds: Rect): ImagePointerPlan {
  if (!validImageBounds(bounds)) { throw new Error('Invalid observed image canvas'); }
  const plan = new ImagePointerPlan(); plan.operation = operation;
  let startX: number[] = [], endX: number[] = [], startY: number[] = [], endY: number[] = [];
  if (operation === 'pinch-out') {
    startX = [0.4, 0.6]; endX = [0.25, 0.75]; startY = [0.5, 0.5]; endY = [0.5, 0.5];
  } else if (operation === 'pinch-in') {
    startX = [0.25, 0.75]; endX = [0.4, 0.6]; startY = [0.5, 0.5]; endY = [0.5, 0.5];
  } else if (operation === 'pinch-moving-focal') {
    startX = [0.4, 0.6]; endX = [0.3, 0.8]; startY = [0.45, 0.45]; endY = [0.55, 0.55];
  } else if (operation === 'pan-two') {
    startX = [0.3, 0.6]; endX = [0.45, 0.75]; startY = [0.45, 0.45]; endY = [0.55, 0.55];
  } else if (operation === 'boundary-pan-two') {
    startX = [0.25, 0.55]; endX = [0.6, 0.9]; startY = [0.35, 0.35]; endY = [0.7, 0.7];
  } else if (operation === 'pan-one') {
    startX = [0.4]; endX = [0.55]; startY = [0.45]; endY = [0.55];
  } else { throw new Error('Unsupported pointer operation'); }
  plan.fingers = startX.length;
  for (let finger = 0; finger < plan.fingers; finger++) {
    const points: Point[] = [];
    for (let step = 0; step < plan.steps; step++) {
      const fraction = step / (plan.steps - 1);
      points.push({
        x: Math.round(bounds.left + (startX[finger] + (endX[finger] - startX[finger]) * fraction) * (bounds.right - bounds.left)),
        y: Math.round(bounds.top + (startY[finger] + (endY[finger] - startY[finger]) * fraction) * (bounds.bottom - bounds.top)),
        displayId: bounds.displayId || 0
      });
    }
    plan.points.push(points);
  }
  return plan;
}

export function imageZoomPercent(text: string): number {
  const normalized = text.trim();
  if (!/^\d{1,3}%$/.test(normalized)) { throw new Error('Zoom text is not observable as an exact percentage'); }
  const value = Number(normalized.substring(0, normalized.length - 1));
  if (value < 100 || value > 250) { throw new Error('Zoom percentage outside the effective Flutter range'); }
  return value;
}
