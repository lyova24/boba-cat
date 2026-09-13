window.addEventListener('DOMContentLoaded', () => {
    const cat = document.getElementById('dummy-cat');
    const catImg = cat.querySelector('img');
    const catNormal = './static/cat-eat_boba.gif';
    const catIDE = './static/cat-eat_dev.gif';
    const tauri = window.__TAURI__;
    const dragThreshold = 4;
    let dragState;
    let suppressNextClick = false;
    let pendingWindowMove;
    let moveInFlight = false;

    const flushWindowPosition = async () => {
        if (moveInFlight || pendingWindowMove === undefined) return;

        const move = pendingWindowMove;
        pendingWindowMove = undefined;
        moveInFlight = true;

        try {
            await tauri.core.invoke('move_cat_along_bottom', move);
        } finally {
            moveInFlight = false;
            if (pendingWindowMove !== undefined) {
                requestAnimationFrame(flushWindowPosition);
            }
        }
    };

    const queueWindowPosition = (x, persist = false) => {
        pendingWindowMove = {
            x,
            persist: persist || pendingWindowMove?.persist || false,
        };
        requestAnimationFrame(flushWindowPosition);
    };

    const updateDragPosition = (state, persist = false) => {
        if (state.windowX === undefined || !state.moved) return;

        const deltaX = state.lastScreenX - state.startScreenX;
        queueWindowPosition(
            Math.round(state.windowX + deltaX * state.scaleFactor),
            persist,
        );
    };

    if (tauri) {
        cat.addEventListener('pointerdown', async (event) => {
            if (event.button !== 0) return;

            const state = {
                pointerId: event.pointerId,
                startScreenX: event.screenX,
                lastScreenX: event.screenX,
                moved: false,
            };
            dragState = state;
            cat.setPointerCapture(event.pointerId);

            try {
                const [windowX, scaleFactor] = await tauri.core.invoke('cat_drag_origin');
                if (dragState !== state && !state.finished) return;

                state.windowX = windowX;
                state.scaleFactor = scaleFactor;
                updateDragPosition(state, state.finished);
            } catch {
                if (dragState === state) {
                    dragState = undefined;
                    cat.classList.remove('dragging');
                }
            }
        });

        cat.addEventListener('pointermove', (event) => {
            const state = dragState;
            if (!state || state.pointerId !== event.pointerId) return;

            state.lastScreenX = event.screenX;
            if (Math.abs(state.lastScreenX - state.startScreenX) >= dragThreshold) {
                state.moved = true;
                cat.classList.add('dragging');
            }
            updateDragPosition(state);
        });

        const finishDragging = (event) => {
            const state = dragState;
            if (!state || state.pointerId !== event.pointerId) return;

            state.finished = true;
            updateDragPosition(state, true);
            dragState = undefined;
            cat.classList.remove('dragging');
            if (cat.hasPointerCapture(event.pointerId)) {
                cat.releasePointerCapture(event.pointerId);
            }

            suppressNextClick = event.type === 'pointerup' && state.moved;
            if (suppressNextClick) {
                setTimeout(() => {
                    suppressNextClick = false;
                }, 0);
            }
        };

        cat.addEventListener('pointerup', finishDragging);
        cat.addEventListener('pointercancel', finishDragging);
    }

    setTimeout(() => {
        cat.classList.add('visible');
    }, Math.random() * (3000 - 100) + 100);

    cat.addEventListener('click', () => {
        if (suppressNextClick) {
            suppressNextClick = false;
            return;
        }

        cat.classList.remove('visible');
        cat.classList.add('hide-right');
        setTimeout(() => {
            cat.classList.remove('hide-right');
            setTimeout(() => {
                cat.classList.add('visible');
            }, 10);
        }, 650);
    });

    cat.addEventListener('contextmenu', (event) => {
        event.preventDefault();
        cat.classList.remove('visible', 'hide-right');
        cat.classList.add('hide-bottom');
        const onTransitionEnd = (transitionEvent) => {
            if (transitionEvent.propertyName === 'bottom') {
                cat.removeEventListener('transitionend', onTransitionEnd);
                tauri?.window.getCurrentWindow().close();
            }
        };
        cat.addEventListener('transitionend', onTransitionEnd);
    });

    if (tauri) {
        let currentIDE = false;
        let desiredIDE = false;
        let animating = false;

        const switchCat = (isIDE) => {
            desiredIDE = isIDE;
            if (currentIDE === desiredIDE || animating) return;

            const nextIDE = desiredIDE;
            animating = true;
            cat.classList.remove('visible', 'hide-bottom');
            cat.classList.add('hide-right');
            setTimeout(() => {
                catImg.src = nextIDE ? catIDE : catNormal;
                currentIDE = nextIDE;
                cat.classList.remove('hide-right');
                setTimeout(() => {
                    cat.classList.add('visible');
                    animating = false;
                    switchCat(desiredIDE);
                }, 10);
            }, 650);
        };

        tauri.event.listen('ide-mode', ({ payload: isIDE }) => {
            switchCat(Boolean(isIDE));
        });
    }
});
