// Continuous HSV + HEX picker. Deliberately no native type=color input and no
// preset swatches. The element's value/input contract matches a form control.
let serial = 0;

function normalizeHex(value) {
  const text = String(value).trim().replace(/^#/, '');
  if (/^[0-9a-f]{3}$/i.test(text)) return '#' + [...text].map(c => c+c).join('').toLowerCase();
  return /^[0-9a-f]{6}$/i.test(text) ? '#'+text.toLowerCase() : null;
}

function toRgb(h, s, v) {
  const c = v*s, x = c*(1-Math.abs((h/60)%2-1)), m = v-c;
  const sectors = [[c,x,0],[x,c,0],[0,c,x],[0,x,c],[x,0,c],[c,0,x]];
  return sectors[Math.floor(h/60)%6].map(channel => Math.round((channel+m)*255));
}

function toHsv(hex, previousHue) {
  const rgb = [1,3,5].map(i => parseInt(hex.slice(i,i+2),16)/255);
  const [r,g,b] = rgb, max = Math.max(...rgb), min = Math.min(...rgb), d = max-min;
  let h = previousHue;
  if (d) h = ((max===r ? (g-b)/d : max===g ? (b-r)/d+2 : (r-g)/d+4)*60+360)%360;
  return [h, max ? d/max : 0, max];
}

class DebugColorPicker extends HTMLElement {
  constructor() {
    super();
    this._hex = '#000000';
    this._h = 0; this._s = 0; this._v = 0;
    this._position = () => this.positionPanel();
  }

  get value() { return this._hex; }
  set value(value) {
    const hex = normalizeHex(value);
    if (!hex) return;
    this._hex = hex;
    [this._h,this._s,this._v] = toHsv(hex,this._h);
    if (this._mounted) this.paint();
  }

  connectedCallback() {
    if (!this._mounted) this.mount();
    window.addEventListener('resize', this._position);
  }

  disconnectedCallback() { window.removeEventListener('resize', this._position); }

  mount() {
    this.innerHTML = `<button type="button" class="color-trigger" aria-haspopup="dialog" aria-expanded="false"><span class="color-swatch" aria-hidden="true"></span><span class="color-hex"></span></button>
      <div class="picker-popover" popover="auto" role="dialog">
        <div class="picker-header"><strong></strong><button type="button" class="picker-close" aria-label="关闭颜色选择器">×</button></div>
        <canvas class="picker-square" width="196" height="128" aria-hidden="true"></canvas>
        <label>色相<input class="picker-hue" type="range" min="0" max="359" step="1" aria-label="色相"></label>
        <label>饱和度<input class="picker-saturation" type="range" min="0" max="100" step="1" aria-label="饱和度"></label>
        <label>明度<input class="picker-value" type="range" min="0" max="100" step="1" aria-label="明度"></label>
        <label>HEX<input class="hex-input" type="text" maxlength="7" spellcheck="false" autocomplete="off" aria-label="HEX 颜色"></label>
        <span class="hex-error" role="status"></span>
      </div>`;
    this._trigger = this.querySelector('.color-trigger');
    this._panel = this.querySelector('.picker-popover');
    this._square = this.querySelector('.picker-square');
    this._text = this.querySelector('.hex-input');
    const label = this.getAttribute('label') || '颜色';
    this._trigger.setAttribute('aria-label', `选择${label}`);
    this._panel.id = `custom-color-${++serial}`;
    this._panel.setAttribute('aria-label', label);
    this._trigger.setAttribute('aria-controls',this._panel.id);
    this.querySelector('strong').textContent = label;
    this._trigger.addEventListener('click', () => {
      if (this._panel.matches(':popover-open')) this._panel.hidePopover();
      else { this._panel.showPopover(); this.positionPanel(); }
    });
    this._panel.addEventListener('toggle', event => this._trigger.setAttribute('aria-expanded', String(event.newState==='open')));
    this._panel.addEventListener('keydown', event => {
      if (event.key !== 'Escape') return;
      event.preventDefault(); event.stopPropagation();
      this._panel.hidePopover(); this._trigger.focus();
    });
    this.querySelector('.picker-close').addEventListener('click', () => { this._panel.hidePopover(); this._trigger.focus(); });
    for (const [selector,field,divisor] of [['.picker-hue','_h',1],['.picker-saturation','_s',100],['.picker-value','_v',100]]) {
      this.querySelector(selector).addEventListener('input', event => {
        event.stopPropagation();
        this[field] = Number(event.target.value)/divisor;
        this.commitHsv();
      });
    }
    this._text.addEventListener('input', event => {
      event.stopPropagation();
      const hex = normalizeHex(this._text.value);
      this._text.setAttribute('aria-invalid', String(!hex));
      this._text.setCustomValidity(hex ? '' : '请输入 #RRGGBB 或 #RGB');
      this.querySelector('.hex-error').textContent = hex ? '' : '请输入 #RRGGBB 或 #RGB';
      if (hex) {
        this._hex = hex;
        [this._h,this._s,this._v] = toHsv(hex,this._h);
        this.paint(false);
        this.dispatchEvent(new Event('input',{bubbles:true}));
      }
      this.positionPanel();
    });
    const point = event => {
      const rect = this._square.getBoundingClientRect();
      this._s = Math.max(0,Math.min(1,(event.clientX-rect.left)/rect.width));
      this._v = 1-Math.max(0,Math.min(1,(event.clientY-rect.top)/rect.height));
      this.commitHsv();
    };
    this._square.addEventListener('pointerdown', event => {
      if (event.button!==0) return;
      event.preventDefault();
      this._square.setPointerCapture(event.pointerId);
      point(event);
    });
    this._square.addEventListener('pointermove', event => {
      if (this._square.hasPointerCapture(event.pointerId)) point(event);
    });
    this._square.addEventListener('pointerup', event => {
      if (this._square.hasPointerCapture(event.pointerId)) this._square.releasePointerCapture(event.pointerId);
    });
    this._mounted = true;
    this.value = this.getAttribute('value') || '#000000';
  }

  commitHsv() {
    this._hex = '#' + toRgb(this._h,this._s,this._v).map(v=>v.toString(16).padStart(2,'0')).join('');
    this.paint();
    this.dispatchEvent(new Event('input',{bubbles:true}));
  }

  paint(updateText = true) {
    this.querySelector('.color-swatch').style.background = this._hex;
    this.querySelector('.color-hex').textContent = this._hex;
    this.querySelector('.picker-hue').value = Math.round(this._h)%360;
    this.querySelector('.picker-saturation').value = Math.round(this._s*100);
    this.querySelector('.picker-value').value = Math.round(this._v*100);
    if (updateText) {
      this._text.value = this._hex;
      this._text.setAttribute('aria-invalid','false');
      this._text.setCustomValidity('');
      this.querySelector('.hex-error').textContent = '';
    }
    const ctx = this._square.getContext('2d'), {width:w,height:h} = this._square;
    ctx.fillStyle = `hsl(${this._h} 100% 50%)`; ctx.fillRect(0,0,w,h);
    const white = ctx.createLinearGradient(0,0,w,0); white.addColorStop(0,'white'); white.addColorStop(1,'#ffffff00');
    ctx.fillStyle = white; ctx.fillRect(0,0,w,h);
    const black = ctx.createLinearGradient(0,0,0,h); black.addColorStop(0,'#00000000'); black.addColorStop(1,'black');
    ctx.fillStyle = black; ctx.fillRect(0,0,w,h);
    const x = this._s*w, y = (1-this._v)*h;
    ctx.beginPath(); ctx.arc(x,y,5,0,Math.PI*2); ctx.strokeStyle='black'; ctx.lineWidth=3; ctx.stroke();
    ctx.strokeStyle='white'; ctx.lineWidth=1.5; ctx.stroke();
  }

  positionPanel() {
    if (!this._panel?.matches(':popover-open')) return;
    const anchor = this._trigger.getBoundingClientRect(), box = this._panel.getBoundingClientRect();
    this._panel.style.left = `${Math.max(8,Math.min(anchor.left,innerWidth-box.width-8))}px`;
    this._panel.style.top = `${Math.max(8,Math.min(anchor.bottom+6,innerHeight-box.height-8))}px`;
  }
}
customElements.define('debug-color-picker',DebugColorPicker);
