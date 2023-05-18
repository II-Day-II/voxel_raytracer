---
layout: post
title: Project outline
tags: [DH2323 Project, GPU Voxel Raytracing]
author: David Kaméus 
---

# Voxel Raytracing

Raytracing is becoming more and more viable as a real-time rendering method, 
with graphics cards getting hardware support for it, and companies like NVIDIA 
constantly pushing the boundaries of what can be done, with improvements in 
raytracing software. But these modern graphics cards are incredibly expensive, 
and NVIDIA's latest algorithms are not readily available. 

It's still possible to do real-time raytracing, but for complicated scenes 
with many triangles, this will require a complicated bounding volume hierarchy 
such as an octtree, in order to achieve a reasonable framerate. If one goes 
beyond raytracing and into path tracing, where rays are not only evaluated 
along their most probable path, but bounced randomly according to the material 
hit, a denoising step is also needed for the image to look decent. 

## Voxels

What I'm going to do in this project is simlar to path tracing, but with 
limited scope: I will be doing it in a voxel world, with lighting calculated 
per voxel.

![Example Voxel image, from Wikipeda](https://upload.wikimedia.org/wikipedia/commons/thumb/b/bc/Voxels.svg/1200px-Voxels.svg.png)

Voxels, sometimes expanded to "Volumetric Pixels", are essentially cubes in a 
3D-grid. 
Think Minecraft if this is abstract to you.
This scope limitation will have a large impact on what kind of scenes can be 
rendered with the program, but it will also reduce the complexity of the 
rendering process by eliminating the denoising step and by nature simplifying the bounding volume hierarchy, allowing very good performance even on limited hardware 
such as laptops without dedicated graphics cards.

## Traversing voxel scenes

Casting a ray through a grid structure can be done very efficiently with the DDA (Digital Differential Analyzer) algorithm. It determines the exact list of grid cells traversed by a ray from its origin, and is used in many different contexts. Determining what pixels make up a line in a rasterization pipeline, and determining the distance to walls in engines similar to that of Wolfenstein 3D are examples related to computer graphics, but it's also used for medicinal purposes in CT scans.

![Example of a wolfenstein style engine where DDA can be used to determine the distance from the camera to the wall](https://upload.wikimedia.org/wikipedia/commons/e/e7/Simple_raycasting_with_fisheye_correction.gif)

Since a voxel world is essentially just a grid, the DDA algorithm can be used to cast rays through it as well.


# Intended outcome

The intended outcome is something similar to this engine created by YouTube user frozein:
<iframe width="560" height="315" src="https://www.youtube-nocookie.com/embed/OAF4RCS_pPc?start=372" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" allowfullscreen></iframe>

In the next post I will be discussing the technologies used and beginning the implementation.