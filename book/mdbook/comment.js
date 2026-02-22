(function() {
    'use strict';

    function insertGiscus() {
        var existingGiscus = document.querySelector('.giscus');
        if (existingGiscus) return;

        var isPrintMode = window.location.search.includes('print');
        if (isPrintMode) return;

        var content = document.querySelector('.content');
        if (!content) return;

        var giscusContainer = document.createElement('div');
        giscusContainer.setAttribute('class', 'giscus');
        giscusContainer.style.cssText = 'margin-top: 2rem; padding-top: 1rem; border-top: 1px solid rgba(255,255,255,0.1);';

        var script = document.createElement('script');
        script.src = 'https://giscus.app/client.js';
        script.setAttribute('data-repo', 'x5zone/minix-rs');
        script.setAttribute('data-repo-id', 'R_kgDORVTPGQ');
        script.setAttribute('data-category', 'General');
        script.setAttribute('data-category-id', 'DIC_kwDORVTPGc4C2_PK');
        script.setAttribute('data-mapping', 'pathname');
        script.setAttribute('data-strict', '0');
        script.setAttribute('data-reactions-enabled', '1');
        script.setAttribute('data-emit-metadata', '0');
        script.setAttribute('data-input-position', 'bottom');
        script.setAttribute('data-theme', 'preferred_color_scheme');
        script.setAttribute('data-lang', 'zh-CN');
        script.setAttribute('crossorigin', 'anonymous');
        script.async = true;

        giscusContainer.appendChild(script);
        content.appendChild(giscusContainer);
    }

    if (typeof mdbook !== 'undefined' && mdbook.onPageLoad) {
        mdbook.onPageLoad(insertGiscus);
    } else {
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', insertGiscus);
        } else {
            insertGiscus();
        }
    }

    window.addEventListener('hashchange', function() {
        setTimeout(function() {
            var existingGiscus = document.querySelector('.giscus');
            if (!existingGiscus) {
                insertGiscus();
            }
        }, 100);
    });
})();
