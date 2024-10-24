# spright

spright is a high performance sprite renderer for wgpu.

It does exactly one thing, and that's draw a lot of sprites to the screen as fast as it can.

## Performance notes

### Minimize the total number of textures

The fewer textures you use, the better. Every time a different texture is used, a different bind group needs to be used and a separate draw call issued.

### Minimize texture switching

Even if you have multiple textures, if they're being drawn together it can still be relatively efficient. However, if e.g. sprites are alternating between textures, then a separate draw call will need to be issued for each texture used. In the worst case, the number of draw calls could be the number of sprites you want to draw!
