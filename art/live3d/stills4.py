import os, json
from render import np, Image, moderngl
import render as R

TL = r'D:\Outputs\CallMe\design\live3d\cast\timelines'
OUT = os.path.join(R.HERE, '..', '..', 'phone', 'ui', 'chats', 'faces')
K, SF, HEAD = 0.42, 2.75, 40
SOLO = {'null': 1.2}
FACES = {'clawd': (300, 250), 'haiku': (300, 250), 'sonnet': (300, 250), 'opus': (300, 250), 'null': (150, 230)}
POSES = ['idle', 'think', 'happy']

if __name__ == '__main__':
    os.makedirs(OUT, exist_ok=True)
    R.render('null', {}, 0.1)
    ctx = R._ctx
    for face, (fw, fh) in FACES.items():
        data = json.load(open(os.path.join(TL, face + '.json')))
        prog = R.program(data['shader'])
        k = K * SOLO.get(face, 1)
        w, h, vh = round(fw * k * SF), round(fh * k * SF), round((fh + HEAD) * k * SF)
        S = 3
        fbo = ctx.simple_framebuffer((w * S, vh * S), components=4)
        vao = ctx.vertex_array(prog, [])
        for pose in POSES:
            prog['uRes'].value = (w * S, h * S)
            for n, v in data['poses'][pose].items():
                if n in prog:
                    prog[n].value = tuple(float(x) for x in v)
            fbo.use()
            fbo.clear(0, 0, 0, 0)
            vao.render(moderngl.TRIANGLES, vertices=3)
            a = np.frombuffer(fbo.read(components=4), np.uint8).reshape(vh * S, w * S, 4)[::-1].astype(np.float32) / 255
            a[..., :3] *= a[..., 3:]
            a = a.reshape(vh, S, w, S, 4).mean(axis=(1, 3))
            a[..., :3] /= np.maximum(a[..., 3:], 1e-6)
            Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8), 'RGBA').save(os.path.join(OUT, f'{face}-{pose}.png'), optimize=True)
            print(face, pose, w, vh)
        vao.release()
        fbo.release()
