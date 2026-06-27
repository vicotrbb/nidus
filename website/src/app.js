const toggle = document.querySelector('.nav-toggle');
const nav = document.querySelector('.site-nav');

if (toggle && nav) {
  const closeNav = () => {
    toggle.setAttribute('aria-expanded', 'false');
    nav.removeAttribute('data-open');
  };

  toggle.addEventListener('click', () => {
    const expanded = toggle.getAttribute('aria-expanded') === 'true';
    toggle.setAttribute('aria-expanded', String(!expanded));
    nav.toggleAttribute('data-open', !expanded);
  });

  nav.addEventListener('click', (event) => {
    if (event.target instanceof HTMLAnchorElement) closeNav();
  });

  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') closeNav();
  });
}

const filter = document.querySelector('#docs-filter');
const links = [...document.querySelectorAll('.docs-link')];
const groups = [...document.querySelectorAll('.docs-nav-group')];
const empty = document.querySelector('.docs-empty');

if (filter && links.length) {
  filter.addEventListener('input', () => {
    const query = filter.value.trim().toLowerCase();
    let visibleCount = 0;

    for (const link of links) {
      const haystack = `${link.dataset.title ?? ''} ${link.dataset.summary ?? ''}`;
      const visible = query.length === 0 || haystack.includes(query);
      link.hidden = !visible;
      if (visible) visibleCount += 1;
    }

    for (const group of groups) {
      group.hidden = ![...group.querySelectorAll('.docs-link')].some((link) => !link.hidden);
    }

    if (empty) empty.hidden = visibleCount > 0;
  });
}

const tocLinks = [...document.querySelectorAll('.docs-toc a')];
const headings = tocLinks
  .map((link) => document.querySelector(decodeURIComponent(link.hash)))
  .filter(Boolean);

if (tocLinks.length && headings.length) {
  const activeById = new Map(tocLinks.map((link) => [link.hash.slice(1), link]));
  const observer = new IntersectionObserver(
    (entries) => {
      const visible = entries
        .filter((entry) => entry.isIntersecting)
        .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
      if (!visible) return;
      for (const link of tocLinks) link.removeAttribute('aria-current');
      activeById.get(visible.target.id)?.setAttribute('aria-current', 'location');
    },
    { rootMargin: '-18% 0px -70% 0px' },
  );

  for (const heading of headings) observer.observe(heading);
}
