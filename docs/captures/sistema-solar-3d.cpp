#include <fstream>
#include <string>

int main() {
    const std::string html_content = R"(
<!DOCTYPE html>
<html lang="es">
<head>
    <meta charset="UTF-8">
    <title>Sistema Solar 3D - Three.js</title>
    <style>
        body { margin: 0; padding: 0; overflow: hidden; font-family: Arial, sans-serif; background: #000; color: white; }
        #info { position: absolute; top: 10px; left: 10px; z-index: 10; max-width: 300px; line-height: 1.4; }
        .control { display: flex; align-items: center; margin: 5px 0; }
        .control label { min-width: 80px; }
        .control input[type="range"] { flex: 1; }
        #controls { position: absolute; bottom: 20px; left: 20px; z-index: 10; background: rgba(0,0,0,0.5); padding: 15px; border-radius: 8px; }
    </style>
</head>
<body>
    <div id="info"></div>
    <div id="controls">
        <div class="control"><label>Velocidad:</label><input type="range" id="speedSlider" min="0.1" max="5" step="0.1" value="1"></div>
        <div class="control"><label>Mostrar órbitas:</label><input type="checkbox" id="orbitToggle" checked></div>
        <div class="control"><label>Mostrar etiquetas:</label><input type="checkbox" id="labelToggle" checked></div>
    </div>

    <script type="importmap">
        {
            "imports": {
                "three": "https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.module.js",
                "three/addons/": "https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/examples/jsm/"
            }
        }
    </script>

    <script type="module">
        import * as THREE from 'three';
        import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

        let scene, camera, renderer, controls;
        const objects = []; // Para almacenar sol, planetas y sus óbiras
        const labels = [];  // Para etiquetas de texto
        const orbitLines = []; // Líneas de órbita

        const solarSystemData = [
            { name: "Sol", radius: 696, color: 0xffcc00, distance: 0, period: 0, texture: null },
            { name: "Mercurio", radius: 2.4, color: 0x8c8c8c, distance: 58, period: 0.24, texture: null },
            { name: "Venus", radius: 6.1, color: 0xf5deb3, distance: 108, period: 0.62, texture: null },
            { name: "Tierra", radius: 6.4, color: 0x4a90e2, distance: 150, period: 1.0, texture: null },
            { name: "Marte", radius: 3.4, color: 0xb22222, distance: 228, period: 1.88, texture: null },
            { name: "Júpiter", radius: 69.9, color: 0xd2b48c, distance: 778, period: 11.86, texture: null },
            { name: "Saturno", radius: 58.2, color: 0xf5deb3, distance: 1433, period: 29.46, texture: null },
            { name: "Urano", radius: 25.4, color: 0xadd8e6, distance: 2872, period: 84.01, texture: null },
            { name: "Neptuno", radius: 24.6, color: 0x4682b4, distance: 4495, period: 164.8, texture: null }
        ];

        init();
        animate();

        function init() {
            scene = new THREE.Scene();
            scene.background = new THREE.Color(0x000000);

            camera = new THREE.PerspectiveCamera(45, window.innerWidth / window.innerHeight, 0.1, 10000);
            camera.position.set(0, 500, 800);

            renderer = new THREE.WebGLRenderer({ antialias: true });
            renderer.setSize(window.innerWidth, window.innerHeight);
            document.body.appendChild(renderer.domElement);

            controls = new OrbitControls(camera, renderer.domElement);
            controls.enableDamping = true;
            controls.dampingFactor = 0.05;
            controls.maxDistance = 2000;
            controls.minDistance = 100;

            // Luz ambiental y direccional
            const ambientLight = new THREE.AmbientLight(0x404040, 0.6);
            scene.add(ambientLight);
            const directionalLight = new THREE.DirectionalLight(0xffffff, 0.8);
            directionalLight.position.set(500, 500, 500);
            scene.add(directionalLight);

            // Crear sol y planetas
            solarSystemData.forEach((body, index) => {
                let geometry, material, mesh;

                if (index === 0) { // Sol
                    geometry = new THREE.SphereGeometry(body.radius, 32, 32);
                    material = new THREE.MeshBasicMaterial({ color: body.color, emissive: body.color, emissiveIntensity: 1 });
                    mesh = new THREE.Mesh(geometry, material);
                    mesh.userData = { name: body.name, radius: body.radius, distance: body.distance, period: body.period, angle: 0 };
                } else {
                    geometry = new THREE.SphereGeometry(body.radius, 24, 24);
                    material = new THREE.MeshStandardMaterial({ color: body.color });
                    mesh = new THREE.Mesh(geometry, material);
                    mesh.userData = { name: body.name, radius: body.radius, distance: body.distance, period: body.period, angle: 0 };

                    // Órbita como línea
                    const orbitGeometry = new THREE.RingGeometry(body.distance - 0.5, body.distance + 0.5, 64);
                    const orbitMaterial = new THREE.MeshBasicMaterial({ color: 0x555555, side: THREE.DoubleSide, transparent: true, opacity: 0.3 });
                    const orbitMesh = new THREE.Mesh(orbitGeometry, orbitMaterial);
                    orbitMesh.rotation.x = Math.PI / 2;
                    scene.add(orbitMesh);
                    orbitLines.push(orbitMesh);
                }

                mesh.position.x = body.distance;
                scene.add(mesh);
                objects.push(mesh);

                // Etiqueta
                const labelDiv = document.createElement('div');
                labelDiv.className = 'label';
                labelDiv.style.position = 'absolute';
                labelDiv.style.color = 'white';
                labelDiv.style.fontSize = '14px';
                labelDiv.style.textShadow = '0 0 5px black';
                labelDiv.style.pointerEvents = 'none';
                labelDiv.textContent = body.name;
                document.body.appendChild(labelDiv);
                labels.push(labelDiv);
            });

            // Manejo de eventos UI
            document.getElementById('speedSlider').addEventListener('input', (e) => {
                window.speedFactor = parseFloat(e.target.value);
            });
            document.getElementById('orbitToggle').addEventListener('change', (e) => {
                orbitLines.forEach(line => line.visible = e.target.checked);
            });
            document.getElementById('labelToggle').addEventListener('change', (e) => {
                labels.forEach(label => label.style.display = e.target.checked ? 'block' : 'none');
            });

            window.speedFactor = 1.0; // Valor inicial

            window.addEventListener('resize', () => {
                camera.aspect = window.innerWidth / window.innerHeight;
                camera.updateProjectionMatrix();
                renderer.setSize(window.innerWidth, window.innerHeight);
            });
        }

        function animate() {
            requestAnimationFrame(animate);

            const speed = window.speedFactor || 1.0;
            const time = Date.now() * 0.0001 * speed; // Tiempo escalado por velocidad

            objects.forEach((obj, index) => {
                if (index === 0) return; // Sol no orbita
                const data = solarSystemData[index];
                if (data.period === 0) return; // Evitar división por cero

                const angle = (time * 2 * Math.PI) / (data.period * 365.25); // Ajustado para años terrestres
                obj.position.x = data.distance * Math.cos(angle);
                obj.position.z = data.distance * Math.sin(angle);
                obj.position.y = 0; // Plano XY

                // Actualizar etiqueta si está activada
                if (labels[index] && labels[index].style.display !== 'none') {
                    const pos = obj.position.clone();
                    pos.project(camera);
                    const x = (pos.x * 0.5 + 0.5) * window.innerWidth;
                    const y = (-pos.y * 0.5 + 0.5) * window.innerHeight;
                    labels[index].style.left = `${x}px`;
                    labels[index].style.top = `${y}px`;
                }
            });

            controls.update();
            renderer.render(scene, camera);
        }
    </script>
</body>
</html>
    )";

    std::ofstream file("sistema_solar_3d.html");
    if (file.is_open()) {
        file << html_content;
        file.close();
    }

    return 0;
}