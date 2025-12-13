import { OrbitControls } from 'three/examples/jsm/Addons.js';
import { Inspector } from 'three/examples/jsm/inspector/Inspector.js';
import Stats from 'three/examples/jsm/libs/stats.module.js';
import { color, cos, float, Fn, instancedArray, sin, uniform, vec4 } from 'three/tsl';
import { AdditiveBlending, Box3, Box3Helper, BoxHelper, CubeTextureLoader, InstancedMesh, PerspectiveCamera, PlaneGeometry, Scene, Sphere, SpriteNodeMaterial, Vector3, WebGPURenderer } from 'three/webgpu';


export class Galaxy {
  camera = new PerspectiveCamera(25, window.innerWidth / window.innerHeight, 0.1, 200)
  scene = new Scene();
  renderer = new WebGPURenderer({
    antialias: true,
    depth: false,
  })

  controls = new OrbitControls(this.camera, this.renderer.domElement)
  targetPosition = new Vector3(0, 0, 0)
  currentPosition = this.targetPosition.clone()

  stats = new Stats()
  
  async init() {
    this.camera.position.set(34.65699659876029, 21.90527423256544, -24.079356892645272);

    this.renderer.setPixelRatio(window.devicePixelRatio);
    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.renderer.setClearColor('#000000');
    document.body.appendChild(this.renderer.domElement);
    
    document.body.appendChild(this.stats.dom)

    await this.renderer.init();

    this.controls.enableDamping = true;
    this.controls.minDistance = 0.1;
    this.controls.maxDistance = 150;
    this.controls.update()

    // this is for you javascript (middle finger)
    const localThis = this
    this.controls.addEventListener('change', this.requestRenderIfNotRequested.bind(localThis))

    window.addEventListener('resize', this.onWindowResize.bind(localThis));

    const cubeTextureLoader = new CubeTextureLoader();
    const texture = await cubeTextureLoader.loadAsync([
      '/skybox/front.png',
      '/skybox/back.png',
      '/skybox/top.png',
      '/skybox/bottom.png',
      '/skybox/left.png',
      '/skybox/right.png',
    ])

    this.scene.background = texture;

    this.loadStars()
  }

  setTarget(target: Vector3) {
    this.targetPosition = target
    this.requestRenderIfNotRequested()
  }

  onWindowResize() {
    this.camera.aspect = window.innerWidth / window.innerHeight;
    this.camera.updateProjectionMatrix();

    this.renderer.setSize(window.innerWidth, window.innerHeight);
    this.requestRenderIfNotRequested()
  }

  renderRequested = false
  render() {
    this.renderRequested = false

    this.controls.update()
    this.stats.update()
    this.renderer.render(this.scene, this.camera)
    

    if (this.currentPosition.distanceToSquared(this.targetPosition) > 0.01) {
      this.currentPosition.lerp(this.targetPosition, 0.08)
      this.controls.target.copy(this.currentPosition)
      this.requestRenderIfNotRequested()
    }
  }

  requestRenderIfNotRequested() {
    if (!this.renderRequested) {
      this.renderRequested = true

      requestAnimationFrame(this.render.bind(this));
    }
  }

  async loadStars() {
    console.log('Loading star data...')
    const starPositionArrays = await Promise.all([0, 1, 2, 3,4,5,6,7,8]
      .map(i => fetch(`/data/neutron_stars${i}.bin`)
        .then(res => res.arrayBuffer())
        .then(arr => new DataView(arr)))
    )

    const count = starPositionArrays.reduce((acc, arr) => acc + arr.byteLength / 12 - 1, 0)
    console.log(`Loaded ${count} stars`)

    for (let i = 0; i < starPositionArrays.length; i++) {
      const arr = starPositionArrays[i];
      
      let aabb_min_x = arr.getFloat32(0, true)
      let aabb_min_y = arr.getFloat32(4, true)
      let aabb_min_z = arr.getFloat32(8, true)
      let aabb_min = new Vector3(aabb_min_x, aabb_min_y, aabb_min_z).divideScalar(1000)
      let aabb_max_x = arr.getFloat32(12, true)
      let aabb_max_y = arr.getFloat32(16, true)
      let aabb_max_z = arr.getFloat32(20, true)
      let aabb_max = new Vector3(aabb_max_x, aabb_max_y, aabb_max_z).divideScalar(1000)
      
      console.log(`Star array ${i}: AABB min(${aabb_min_x}, ${aabb_min_y}, ${aabb_min_z}) max(${aabb_max_x}, ${aabb_max_y}, ${aabb_max_z})`)
      let starArr = new Float32Array(arr.buffer, 3*4)
      const positionBuffer = instancedArray(starArr, 'vec3');

      // nodes
      const material = new SpriteNodeMaterial({ blending: AdditiveBlending, depthWrite: false, depthTest: false });
      const colorA = uniform(color('#5900ff'));

      new Vector3()
      material.positionNode = positionBuffer.toAttribute().div(float(1000));

      material.colorNode = Fn(() => {
        let c = sin(i);
        let c2 = cos(i);
        return vec4(colorA, 1);
      })();

      material.scaleNode = float(0.05);

      // mesh
      const geometry = new PlaneGeometry(0.5, 0.5);
      const mesh = new InstancedMesh(geometry, material, starArr.length / 3);
      mesh.boundingBox = new Box3(aabb_min, aabb_max)
      const boundingSphere = new Sphere()
      mesh.boundingBox.getBoundingSphere(boundingSphere)
      mesh.boundingSphere = boundingSphere
      // let box = new Box3Helper(mesh.boundingBox!)
      // this.scene.add(box)
      mesh.frustumCulled = true
      this.scene.add(mesh);
      this.requestRenderIfNotRequested()
    }
  }
}
