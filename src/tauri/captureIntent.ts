/**
 * 标记「这次截屏只是给已有图片重设 ROI」。
 *
 * 截屏统一会入库一张新图并广播 screen-capture-saved。图片库抽屉在重设 ROI 时
 * 会把这临时图删掉，但同一时刻积木块的图片选择器也订阅了同一个事件——它会把
 * 块配置改成那张即将被删除的临时图 id，保存后执行就报「图片不存在」。
 * 这个标记让选择器知道「这次不是给块换图」。
 */

let roiPickActive = false;

export function beginRoiPick(): void {
  roiPickActive = true;
}

export function endRoiPick(): void {
  roiPickActive = false;
}

export function isRoiPick(): boolean {
  return roiPickActive;
}
