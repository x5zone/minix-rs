(function() {
    'use strict';

    var GISCUS_CONFIG = {
        repo: 'YOUR_USERNAME/YOUR_REPO',
        repoId: 'YOUR_REPO_ID',
        category: 'Announcements',
        categoryId: 'YOUR_CATEGORY_ID',
        mapping: 'pathname',
        term: '',
        strict: '0',
        reactionsEnabled: '1',
        emitMetadata: '0',
        inputPosition: 'top',
        theme: 'navy_dimmed',
        lang: 'zh-CN',
        loading: 'lazy'
    };

    function insertGiscus() {
        var pageWrapper = document.querySelector('.page-wrapper');
        if (!pageWrapper) return;

        var existingGiscus = document.getElementById('giscus-container');
        if (existingGiscus) return;

        var isPrintMode = window.location.search.includes('print');
        if (isPrintMode) return;

        var giscusContainer = document.createElement('div');
        giscusContainer.id = 'giscus-container';
        giscusContainer.style.cssText = 'margin-top: 2rem; padding-top: 1rem; border-top: 1px solid rgba(255,255,255,0.1);';

        var giscusFrame = document.createElement('iframe');
        giscusFrame.src = 'https://giscus.app/' + buildQueryString(GISCUS_CONFIG);
        giscusFrame.setAttribute('width', '100%');
        giscusFrame.setAttribute('height', '400');
        giscusFrame.style.cssText = 'border: none; background-color: transparent;';
        giscusFrame.title = 'Comments (Giscus)';

        giscusContainer.appendChild(giscusFrame);

        var content = document.querySelector('.content');
        if (content) {
            content.appendChild(giscusContainer);
        }
    }

    function buildQueryString(config) {
        var params = [];
        for (var key in config) {
            if (config.hasOwnProperty(key) && config[key] !== '') {
                params.push(encodeURIComponent(key) + '=' + encodeURIComponent(config[key]));
            }
        }
        return '?' + params.join('&');
    }

    function init() {
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', insertGiscus);
        } else {
            insertGiscus();
        }
    }

    if (typeof mdbook !== 'undefined' && mdbook.onPageLoad) {
        mdbook.onPageLoad(insertGiscus);
    } else {
        init();
    }

    window.addEventListener('hashchange', function() {
        setTimeout(insertGiscus, 100);
    });
})();
