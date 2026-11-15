/**
 * The Model tab's 3D view (addendum 7.20): three.js (MIT) with GLTFLoader and OrbitControls.
 * Imported only by `ModelViewer.svelte`, which the Explorer loads on demand, so three.js is a
 * chunk of its own and the main bundle does not grow.
 *
 * - Draws only when something changes (orbit, resize, a new model): no render loop while the
 *   game may be running.
 * - Every mesh gets one shared grey material (a Print shell has none); wireframe is a switch on it.
 * - The size readout is the bounding box of the vertices the triangles use, with node transforms
 *   (StarBreaker meshes share vertex buffers, so per-geometry boxes are far too large).
 * - The GLB is parsed in memory (`GLTFLoader.parse`): no URL, fetch or blob, so the CSP needs
 *   nothing new.
 */
import {
  AmbientLight,
  Box3,
  BufferGeometry,
  Color,
  DirectionalLight,
  DoubleSide,
  HemisphereLight,
  Material,
  Mesh,
  MeshStandardMaterial,
  Object3D,
  PerspectiveCamera,
  Scene,
  Sphere,
  Vector3,
  WebGLRenderer,
} from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';

export interface ModelStats {
  triangles: number;
  meshes: number;
  /** Bounding box size in metres: x (width), y (height), z (length); glTF is Y-up. */
  size: [number, number, number];
}

/** Colours per theme (the canvas is transparent: the card behind it shows through). */
const TONE = {
  dark: { model: 0xb9c4d4, sky: 0xdbe6ff, ground: 0x1a2030 },
  light: { model: 0x7d8796, sky: 0xffffff, ground: 0xb8bcc4 },
};

/** The box of the vertices the triangles reference, in world space. */
export function exactBox(root: Object3D): Box3 {
  root.updateMatrixWorld(true);
  const box = new Box3();
  const v = new Vector3();
  root.traverse((o) => {
    const m = o as Mesh;
    if (!m.isMesh) return;
    const g = m.geometry as BufferGeometry;
    const pos = g.getAttribute('position');
    if (!pos) return;
    const idx = g.getIndex();
    const n = idx ? idx.count : pos.count;
    for (let i = 0; i < n; i++) {
      v.fromBufferAttribute(pos, idx ? idx.getX(i) : i).applyMatrix4(m.matrixWorld);
      box.expandByPoint(v);
    }
  });
  return box;
}

export function countTriangles(root: Object3D): { triangles: number; meshes: number } {
  let triangles = 0;
  let meshes = 0;
  root.traverse((o) => {
    const m = o as Mesh;
    if (!m.isMesh) return;
    meshes++;
    const g = m.geometry as BufferGeometry;
    const n = g.getIndex()?.count ?? g.getAttribute('position')?.count ?? 0;
    triangles += Math.floor(n / 3);
  });
  return { triangles, meshes };
}

/** Parse a GLB held in memory. */
export async function parseGlb(data: ArrayBuffer): Promise<Object3D> {
  const gltf = await new GLTFLoader().parseAsync(data, '');
  return gltf.scene;
}

export class ModelScene {
  readonly renderer: WebGLRenderer;
  private scene = new Scene();
  private camera = new PerspectiveCamera(40, 1, 0.1, 1000);
  private controls: OrbitControls;
  private material: MeshStandardMaterial;
  private hemi: HemisphereLight;
  private model: Object3D | null = null;
  private fit: Sphere | null = null;
  private frame = 0;
  private resize: ResizeObserver;
  private disposed = false;
  /** Pixels at the bottom covered by the readouts: the model is fitted above them. */
  private inset = 0;
  onContextLost: () => void = () => {};

  /** Throws when WebGL is not available (the caller shows `webglProblem`). */
  constructor(
    private host: HTMLElement,
    theme: 'dark' | 'light',
  ) {
    this.renderer = new WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'default' });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    this.renderer.setClearColor(0x000000, 0);
    const c = this.renderer.domElement;
    c.style.display = 'block';
    c.style.width = '100%';
    c.style.height = '100%';
    c.style.touchAction = 'none';
    c.setAttribute('aria-hidden', 'true');
    host.appendChild(c);
    c.addEventListener('webglcontextlost', (e) => {
      e.preventDefault();
      this.onContextLost();
    });
    const t = TONE[theme];
    this.material = new MeshStandardMaterial({ color: t.model, metalness: 0.15, roughness: 0.62, side: DoubleSide });
    this.hemi = new HemisphereLight(t.sky, t.ground, 1.6);
    this.scene.add(this.hemi, new AmbientLight(0xffffff, 0.25));
    const key = new DirectionalLight(0xffffff, 1.8);
    key.position.set(1, 1.4, 0.8);
    this.camera.add(key);
    this.scene.add(this.camera);
    this.controls = new OrbitControls(this.camera, c);
    this.controls.enableDamping = false;
    this.controls.screenSpacePanning = true;
    this.controls.addEventListener('change', () => this.draw());
    this.resize = new ResizeObserver(() => this.sizeToHost());
    this.resize.observe(host);
    this.sizeToHost();
  }

  setTheme(theme: 'dark' | 'light'): void {
    const t = TONE[theme];
    this.material.color = new Color(t.model);
    this.hemi.color = new Color(t.sky);
    this.hemi.groundColor = new Color(t.ground);
    this.draw();
  }

  setWireframe(on: boolean): void {
    this.material.wireframe = on;
    this.draw();
  }

  /** Show `root` (replacing the last model), fitted to the view; returns its readouts. */
  show(root: Object3D): ModelStats {
    this.clearModel();
    root.traverse((o) => {
      const m = o as Mesh;
      if (!m.isMesh) return;
      const g = m.geometry as BufferGeometry;
      if (!g.getAttribute('normal')) g.computeVertexNormals();
      const old = m.material;
      for (const x of Array.isArray(old) ? old : [old]) (x as Material).dispose();
      m.material = this.material;
    });
    const box = exactBox(root);
    const size = box.getSize(new Vector3());
    this.fit = box.isEmpty() ? new Sphere(new Vector3(), 1) : box.getBoundingSphere(new Sphere());
    this.model = root;
    this.scene.add(root);
    this.resetView();
    return { ...countTriangles(root), size: [size.x, size.y, size.z] };
  }

  /** Keep the model clear of `px` at the bottom (the readouts drawn over the canvas). */
  setInset(px: number): void {
    const v = Math.max(0, Math.round(px));
    if (v === this.inset) return;
    this.inset = v;
    this.sizeToHost();
    if (this.fit) this.resetView();
  }

  /** A three-quarter view from the front left, the whole model in sight above the readouts. */
  resetView(): void {
    const s = this.fit ?? new Sphere(new Vector3(), 10);
    const r = Math.max(s.radius, 0.01);
    const w = Math.max(1, this.host.clientWidth);
    const h = Math.max(1, this.host.clientHeight);
    const inset = Math.min(this.inset, h / 2);
    // The camera's fov spans h + inset (the view is offset by the inset); fit the sphere in
    // the h - inset above the readouts and in the width.
    const t = Math.tan((this.camera.fov * Math.PI) / 360);
    const half = Math.min(Math.atan((t * (h - inset)) / (h + inset)), Math.atan((t * w) / (h + inset)));
    // The bounding sphere is loose for a three-quarter view of a long, flat ship: 0.9 of it.
    const dist = (r / Math.sin(half)) * 0.9;
    const dir = new Vector3(-0.9, 0.55, -1).normalize();
    this.camera.near = Math.max(r / 500, 0.01);
    this.camera.far = r * 50 + dist;
    this.camera.position.copy(s.center).addScaledVector(dir, dist);
    this.camera.updateProjectionMatrix();
    this.controls.target.copy(s.center);
    this.controls.minDistance = r * 0.05;
    this.controls.maxDistance = r * 20;
    this.controls.update();
    this.draw();
  }

  private sizeToHost(): void {
    const w = Math.max(1, this.host.clientWidth);
    const h = Math.max(1, this.host.clientHeight);
    const inset = Math.min(this.inset, h / 2);
    this.renderer.setSize(w, h, false);
    // A taller virtual view, of which the bottom h is shown: its centre sits above the readouts.
    this.camera.aspect = w / (h + inset);
    if (inset > 0) this.camera.setViewOffset(w, h + inset, 0, inset, w, h);
    else this.camera.clearViewOffset();
    this.camera.updateProjectionMatrix();
    this.draw();
  }

  /** One frame at the next animation frame (coalesced). */
  draw(): void {
    if (this.disposed || this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      if (!this.disposed) this.renderer.render(this.scene, this.camera);
    });
  }

  /** Draw now (headless renders and tests). */
  drawNow(): void {
    if (!this.disposed) this.renderer.render(this.scene, this.camera);
  }

  private clearModel(): void {
    if (!this.model) return;
    this.scene.remove(this.model);
    this.model.traverse((o) => {
      const m = o as Mesh;
      if (m.isMesh) (m.geometry as BufferGeometry).dispose();
    });
    this.model = null;
  }

  dispose(): void {
    this.disposed = true;
    if (this.frame) cancelAnimationFrame(this.frame);
    this.resize.disconnect();
    this.controls.dispose();
    this.clearModel();
    this.material.dispose();
    this.renderer.dispose();
    this.renderer.forceContextLoss();
    this.renderer.domElement.remove();
  }
}
