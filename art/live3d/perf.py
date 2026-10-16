import re, time, json, os
from render import program, render, SIZE, _ctx, VERT
import render as R
import moderngl
src = open(os.path.join(R.HERE, 'clawd_cast.frag'), encoding='utf8').read().split('\n', 1)[1]
alive = '#version 330\n#define MAT_ID 0\n' + src.replace('precision highp float;', '', 1)
render('null', {}, 1)
ctx = R._ctx
cases = [('app vinyl Clawd', ctx.program(vertex_shader=VERT, fragment_shader=alive), (300, 250), {'uBody': (1, 0, 0, 0), 'uArms': (-0.5, -0.5, 0, 0), 'uEyes': (0, 0, 1, 0)})]
for m, n in enumerate(['vinyl', 'cartoon', 'plush', 'chrome']):
    cases.append((f'cast {n}', program('clawd_cast'), SIZE['clawd_cast'], {'uBody': (1, 0, 0, 0), 'uArms': (-0.5, -0.5, 0, 0), 'uEyes': (0, 0, 1, 0), 'uMat': (m, 0, 0, 0)}))
cases.append(('null', program('null'), SIZE['null'], {'uBody': (1, 0, 0, 0), 'uEyes': (0, 0, 1, 0), 'uMood': (0, 1, 0, 3), 'uHands': (0, 0, 0.5, 1)}))
S = 2.75
base = None
for name, prog, size, u in cases:
    w, h = round(size[0] * S), round(size[1] * S)
    fbo = ctx.simple_framebuffer((w, h), components=4)
    vao = ctx.vertex_array(prog, [])
    prog['uRes'].value = (w, h)
    for k, v in u.items():
        if k in prog: prog[k].value = v
    fbo.use()
    for _ in range(20): vao.render(moderngl.TRIANGLES, vertices=3)
    ctx.finish()
    q = ctx.query(time=True)
    with q:
        for _ in range(300): vao.render(moderngl.TRIANGLES, vertices=3)
    ms = q.elapsed / 300 / 1e6
    base = base or ms
    print(f'{name:16s} {w}x{h}  {ms:.3f} ms/frame on this GPU  x{ms / base:.2f} of app Clawd')
print('-- with the material fixed at compile time (#define MAT_ID n):')
src = open(os.path.join(R.HERE, 'clawd_cast.frag'), encoding='utf8').read().replace('#version 300 es', '#version 330', 1).replace('precision highp float;', '', 1)
for m, n in enumerate(['vinyl', 'cartoon', 'plush', 'chrome']):
    prog = ctx.program(vertex_shader=VERT, fragment_shader=src.replace('#version 330', f'#version 330\n#define MAT_ID {m}', 1))
    w, h = round(300 * S), round(250 * S)
    fbo = ctx.simple_framebuffer((w, h), components=4); vao = ctx.vertex_array(prog, [])
    prog['uRes'].value = (w, h)
    for k, v in {'uBody': (1, 0, 0, 0), 'uArms': (-0.5, -0.5, 0, 0), 'uEyes': (0, 0, 1, 0)}.items(): prog[k].value = v
    fbo.use()
    for _ in range(20): vao.render(moderngl.TRIANGLES, vertices=3)
    ctx.finish(); q = ctx.query(time=True)
    with q:
        for _ in range(300): vao.render(moderngl.TRIANGLES, vertices=3)
    ms = q.elapsed / 300 / 1e6
    print(f'cast {n:11s} {w}x{h}  {ms:.3f} ms/frame  x{ms / base:.2f} of app Clawd')
