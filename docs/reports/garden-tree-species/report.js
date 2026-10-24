'use strict';

const cards = [...document.querySelectorAll('[data-seed-card]')];
const filters = document.querySelector('.filters');
const status = document.getElementById('gallery-status');
function selectSeed(seed) {
  let visible = 0;
  for (const card of cards) {
    card.hidden = seed !== 'all' && card.dataset.seedCard !== seed;
    if (!card.hidden) visible += 1;
  }
  for (const button of filters.querySelectorAll('button')) {
    button.setAttribute('aria-pressed', String(button.dataset.seed === seed));
  }
  status.textContent = seed === 'all'
    ? `显示全部 ${visible} 张：三个树种 × 三个种子。点击图片查看原始全帧。`
    : `当前种子 ${seed} · ${visible} 个树种。点击图片查看原始全帧。`;
}
for (const button of filters.querySelectorAll('button')) {
  button.addEventListener('click', () => selectSeed(button.dataset.seed));
}
filters.hidden = false;
selectSeed('122');

const dialog = document.getElementById('image-dialog');
const fullImage = document.getElementById('full-image');
let opener;
for (const button of document.querySelectorAll('[data-zoom]')) {
  button.addEventListener('click', () => {
    const image = button.querySelector('img');
    opener = button;
    fullImage.src = image.src;
    fullImage.alt = image.alt;
    document.getElementById('image-title').textContent = button.getAttribute('aria-label');
    dialog.showModal();
  });
}
document.getElementById('close-image').addEventListener('click', () => dialog.close());
dialog.addEventListener('close', () => {
  fullImage.removeAttribute('src');
  opener?.focus({ preventScroll: true });
});
