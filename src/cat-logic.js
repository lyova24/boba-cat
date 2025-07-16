window.addEventListener('DOMContentLoaded', () => {
    const cat = document.getElementById('dummy-cat');
    const catImg = cat.querySelector('img');
    const catNormal = './static/cat-eat_boba.gif';
    const catIDE = './static/cat-eat_dev.gif';

    setTimeout(() => {
        cat.classList.add('visible');
    }, Math.random() * (3000 - 100) + 100);

    cat.addEventListener('click', () => {
        cat.classList.remove('visible');
        cat.classList.add('hide-right');
        setTimeout(() => {
            cat.classList.remove('hide-right');
            setTimeout(() => {
                cat.classList.add('visible');
            }, 10);
        }, 650);
    });

    cat.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        cat.classList.remove('visible', 'hide-right');
        cat.classList.add('hide-bottom');
        const onTransitionEnd = (event) => {
            if (event.propertyName === 'bottom') {
                if (window.electronAPI && window.electronAPI.closeApp) {
                    window.electronAPI.closeApp();
                }
                cat.removeEventListener('transitionend', onTransitionEnd);
            }
        };
        cat.addEventListener('transitionend', onTransitionEnd);
    });

    if (window.electronAPI && window.electronAPI.onIDEStatus) {
        let currentIDE;
        let animating = false;
        window.electronAPI.onIDEStatus((isIDE) => {
            if (currentIDE === isIDE || animating) return;
            currentIDE = isIDE;
            animating = true;
            cat.classList.remove('visible', 'hide-bottom');
            cat.classList.add('hide-right');
            setTimeout(() => {
                catImg.src = isIDE ? catIDE : catNormal;
                cat.classList.remove('hide-right');
                setTimeout(() => {
                    cat.classList.add('visible');
                    animating = false;
                }, 10);
            }, 650);
        });
    }
});
