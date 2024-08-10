export class Viewport {
    constructor() {
        this.panX = 0;
        this.panY = 0;
        this.zoomLevel = 0;
    }

    get zoom() {
        return Math.pow(2, this.zoomLevel * 0.25);
    }

    panAround(x, y) {
        this.panX -= x / this.zoom;
        this.panY -= y / this.zoom;
    }

    zoomIn(delta) {
        this.zoomLevel += delta;
        this.zoomLevel = Math.max(-16, Math.min(20, this.zoomLevel));
    }

    getVisibleRect(windowSize) {
        let invZoom = 1 / this.zoom;
        let width = windowSize.width * invZoom;
        let height = windowSize.height * invZoom;
        return {
            x: this.panX - width / 2,
            y: this.panY - height / 2,
            width,
            height,
        };
    }

    toViewportSpace(x, y, windowSize) {
        return [
            (x - windowSize.width / 2) / this.zoom + this.panX,
            (y - windowSize.height / 2) / this.zoom + this.panY,
        ];
    }

    toScreenSpace(x, y, windowSize) {
        return [
            (x - this.panX) * this.zoom + windowSize.width / 2,
            (y - this.panY) * this.zoom + windowSize.height / 2,
        ];
    }
}
