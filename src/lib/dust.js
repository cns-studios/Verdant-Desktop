const PARTICLES_PER_ROW = 30;
const TINTS = ["rgba(255,255,255,0.95)", "rgba(255,214,120,0.9)", "rgba(175,225,255,0.9)"];
const FADE_SHARE = 0.25;

function spawnParticles(node) {
  const rect = node.getBoundingClientRect();
  const baseColor = getComputedStyle(node).color || "rgb(90,94,86)";
  const layer = document.createElement("div");
  layer.className = "dust-layer";
  document.body.appendChild(layer);

  const finished = [];
  for (let i = 0; i < PARTICLES_PER_ROW; i++) {
    const particle = document.createElement("div");
    const size = 1.5 + Math.random() * 3.5;
    particle.className = "dust-particle";
    particle.style.cssText = `left:${rect.left + Math.random() * rect.width}px;top:${rect.top + Math.random() * rect.height}px;width:${size}px;height:${size}px;background:${TINTS[i % 4] ?? baseColor}`;
    layer.appendChild(particle);

    const dx = (Math.random() - 0.5) * 110;
    const dy = -15 - Math.random() * 80;
    const rotation = (Math.random() - 0.5) * 200;
    const scale = 0.05 + Math.random() * 0.35;
    finished.push(
      particle.animate(
        [
          { transform: "translate(0,0) scale(1)", opacity: 1 },
          { transform: `translate(${dx}px, ${dy}px) rotate(${rotation}deg) scale(${scale})`, opacity: 0 },
        ],
        { duration: 550 + Math.random() * 400, easing: "cubic-bezier(0.15, 0.6, 0.35, 1)", fill: "forwards" },
      ).finished,
    );
  }
  Promise.allSettled(finished).then(() => layer.remove());
}

export function dust(node, { removed }) {
  if (!removed()) return { duration: 0 };
  spawnParticles(node);

  const style = getComputedStyle(node);
  const height = node.offsetHeight;
  const paddingTop = parseFloat(style.paddingTop) || 0;
  const paddingBottom = parseFloat(style.paddingBottom) || 0;

  return {
    duration: 550,
    css: (t) => {
      const opacity = Math.max(0, (t - (1 - FADE_SHARE)) / FADE_SHARE);
      const size = Math.min(1, t / (1 - FADE_SHARE));
      const eased = size * size * (3 - 2 * size);
      return `opacity:${opacity};height:${height * eased}px;padding-top:${paddingTop * eased}px;padding-bottom:${paddingBottom * eased}px;overflow:hidden;pointer-events:none;`;
    },
  };
}
