/**
 * atlassian-cli Website Interactions
 */

(function() {
    'use strict';

    // ========================================
    // Terminal Typing Animation
    // ========================================
    function initTerminalAnimation() {
        const lines = document.querySelectorAll('.terminal-line');
        if (!lines.length) return;

        lines.forEach((line, index) => {
            const delay = parseInt(line.dataset.delay) || index * 2000;
            const textEl = line.querySelector('.terminal-text');
            const fullText = textEl?.dataset.text || '';

            // Hide initially
            line.style.opacity = '0';

            setTimeout(() => {
                line.style.opacity = '1';
                typeText(textEl, fullText);
            }, delay);
        });
    }

    function typeText(element, text) {
        if (!element) return;

        let i = 0;
        const speed = 30;
        element.textContent = '';

        // Add cursor
        const cursor = document.createElement('span');
        cursor.className = 'terminal-cursor';
        element.appendChild(cursor);

        function type() {
            if (i < text.length) {
                element.textContent = text.substring(0, i + 1);
                element.appendChild(cursor);
                i++;
                setTimeout(type, speed);
            } else {
                // Remove cursor after typing
                setTimeout(() => {
                    cursor.remove();
                }, 1000);
            }
        }

        type();
    }

    // ========================================
    // Tab Switching
    // ========================================
    function initTabs() {
        const tabButtons = document.querySelectorAll('.tab-btn');
        const tabPanels = document.querySelectorAll('.example-panel');

        tabButtons.forEach(btn => {
            btn.addEventListener('click', () => {
                const targetTab = btn.dataset.tab;

                // Update buttons
                tabButtons.forEach(b => b.classList.remove('active'));
                btn.classList.add('active');

                // Update panels
                tabPanels.forEach(panel => {
                    panel.classList.remove('active');
                    if (panel.id === `tab-${targetTab}`) {
                        panel.classList.add('active');
                    }
                });
            });
        });
    }

    // ========================================
    // Copy to Clipboard
    // ========================================
    function initCopyButtons() {
        document.querySelectorAll('.copy-btn').forEach(btn => {
            btn.addEventListener('click', async (e) => {
                e.preventDefault();

                let textToCopy = '';

                // Check for data-copy-target (for code blocks)
                const targetId = btn.dataset.copyTarget;
                if (targetId) {
                    const targetEl = document.getElementById(targetId);
                    textToCopy = targetEl?.textContent || '';
                }

                // Check for parent's data-copy attribute
                const parent = btn.closest('[data-copy]');
                if (parent) {
                    textToCopy = parent.dataset.copy;
                }

                if (textToCopy) {
                    try {
                        await navigator.clipboard.writeText(textToCopy.trim());
                        showToast('Copied!');

                        // Visual feedback
                        btn.classList.add('copied');
                        setTimeout(() => btn.classList.remove('copied'), 1500);
                    } catch (err) {
                        console.error('Copy failed:', err);
                        showToast('Failed to copy');
                    }
                }
            });
        });
    }

    // ========================================
    // Toast Notifications
    // ========================================
    function showToast(message) {
        const toast = document.getElementById('toast');
        if (!toast) return;

        toast.textContent = message;
        toast.classList.add('show');

        setTimeout(() => {
            toast.classList.remove('show');
        }, 2000);
    }

    // ========================================
    // Smooth Scroll for Anchor Links
    // ========================================
    function initSmoothScroll() {
        document.querySelectorAll('a[href^="#"]').forEach(anchor => {
            anchor.addEventListener('click', (e) => {
                const targetId = anchor.getAttribute('href');
                if (targetId === '#') return;

                const targetEl = document.querySelector(targetId);
                if (targetEl) {
                    e.preventDefault();
                    const navHeight = document.querySelector('.nav')?.offsetHeight || 64;
                    const targetPosition = targetEl.offsetTop - navHeight - 20;

                    window.scrollTo({
                        top: targetPosition,
                        behavior: 'smooth'
                    });
                }
            });
        });
    }

    // ========================================
    // Scroll Reveal Animation
    // ========================================
    function initScrollReveal() {
        const observer = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                if (entry.isIntersecting) {
                    entry.target.classList.add('revealed');
                    observer.unobserve(entry.target);
                }
            });
        }, {
            threshold: 0.1,
            rootMargin: '0px 0px -50px 0px'
        });

        // Observe sections
        document.querySelectorAll('.product-card, .feature-card, .install-card').forEach(el => {
            el.style.opacity = '0';
            el.style.transform = 'translateY(20px)';
            el.style.transition = 'opacity 0.5s ease, transform 0.5s ease';
            observer.observe(el);
        });
    }

    // Add revealed class styles dynamically
    function addRevealStyles() {
        const style = document.createElement('style');
        style.textContent = `
            .revealed {
                opacity: 1 !important;
                transform: translateY(0) !important;
            }
        `;
        document.head.appendChild(style);
    }

    // ========================================
    // Initialize
    // ========================================
    function init() {
        addRevealStyles();
        initTerminalAnimation();
        initTabs();
        initCopyButtons();
        initSmoothScroll();
        initScrollReveal();
    }

    // Run on DOM ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();
