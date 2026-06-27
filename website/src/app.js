const toggle = document.querySelector('.nav-toggle');
const nav = document.querySelector('.site-nav');

if (toggle && nav) {
  toggle.addEventListener('click', () => {
    const expanded = toggle.getAttribute('aria-expanded') === 'true';
    toggle.setAttribute('aria-expanded', String(!expanded));
    nav.toggleAttribute('data-open', !expanded);
  });
}

const filter = document.querySelector('#docs-filter');
const links = [...document.querySelectorAll('.docs-links a')];

if (filter && links.length) {
  filter.addEventListener('input', () => {
    const query = filter.value.trim().toLowerCase();
    for (const link of links) {
      link.hidden = query.length > 0 && !link.textContent.toLowerCase().includes(query);
    }
  });
}
