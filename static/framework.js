export function listen(...listenerSpecs) {
    return new Promise((resolve) => {
        let removeAllEventListeners;

        let listeners = listenerSpecs.map(([element, eventName]) => {
            let listener = (event) => {
                removeAllEventListeners();
                resolve(event);
            };
            element.addEventListener(eventName, listener);
            return { element, eventName, func: listener };
        });

        removeAllEventListeners = () => {
            for (let listener of listeners) {
                listener.element.removeEventListener(listener.eventName, listener.func);
            }
        };
    });
}

export function debounce(time, fn) {
    let timeout = null;
    return (...args) => {
        if (timeout == null) {
            fn(...args);
            timeout = setTimeout(() => (timeout = null), time);
        }
    };
}
