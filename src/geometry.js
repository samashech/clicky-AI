export function localPoint(point, origin, scale) {
    return { x: (point.x-origin[0])/scale, y: (point.y-origin[1])/scale };
}
export function curve(point, target) {
    const x = target.x + target.width/2, y = target.y + target.height/2;
    const middle = (point.x+x)/2;
    return `M ${point.x} ${point.y} C ${middle} ${point.y}, ${middle} ${y}, ${x} ${y}`;
}
