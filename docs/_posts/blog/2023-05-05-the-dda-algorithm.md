---
layout: post
title: The DDA algorithm
tags: [DH2323 Project, GPU Voxel Raytracing]
author: David Kaméus 
---

# Voxels on the GPU

First things first, I need to get some voxel data onto the GPU so that I have something to draw. But I need to be careful, since GPU memory is rather limited, especially on older and cheaper graphics cards. 
For now, I just send an array of integers, where 1 represents a voxel and 0 represents an absence of a voxel, as well as a triple (x, y, z) representing the size of the scene in each direction.
This doesn't include any color information, so the scenes I can draw won't be particulary pretty. But for now, it's enough to get to implementing the main focus of this project.

# Traversing a 3D voxel space

The DDA algorithm is rather simple. [This video](https://www.youtube.com/watch?v=NbSee-XM7WA) by javidx9 explains it quite clearly in a 2D environment. Expanding it to 3D is 
not particularly difficult. 
[This paper](https://www.researchgate.net/publication/233899848_Efficient_implementation_of_the_3D-DDA_ray_traversal_algorithm_on_GPU_and_its_application_in_radiation_dose_calculation) 
by Xiao et al. provides an even faster version that eliminates the branching paths used in standard implementations. [This shadertoy implementation](https://www.shadertoy.com/view/4dX3zl) shows how it can be used for voxel raytracing.
Removing if-statements may not seem like a big deal for a simple code such as this, but code running on the GPU is different. GPUs are designed to run as fast as possible, so they don't really have time to wait for the program to tell them what branch to choose before loading the next instruction. This means they may need to run both branches, and then undo what was not supposed to be done once it finds out what that was. Which takes a bit of extra time, that can add up when there are a lot of branches.
The wgsl implementation I'm using looks a little confusing, since I'm using pointers, but here's an excerpt:
```wgsl
// initialize a DDA cursor that can be stepped through the world
fn init_DDA(ray: Ray) -> DDA {
    var dda_initial: DDA;
    dda_initial.ray = ray;
    dda_initial.pos = vec3<i32>(floor(ray.position));
    dda_initial.delta_dist = abs(ray.inv_direction);
    dda_initial.step_dir = vec3<i32>(sign(ray.direction));
    dda_initial.side_dist = (sign(ray.direction) * (vec3<f32>(dda_initial.pos) - ray.position) + (sign(ray.direction) * 0.5) + 0.5) * dda_initial.delta_dist;
    return dda_initial;
}
// steps the DDA state one unit along the initial ray direction. Returns the normal of the voxel that was "hit"
fn step_DDA(state: ptr<function, DDA>) -> vec3<f32> {
    let mask: vec3<bool> = (*state).side_dist.xyz <= min((*state).side_dist.yzx, (*state).side_dist.zxy);
    (*state).side_dist += vec3<f32>(mask) * (*state).delta_dist;
    (*state).pos += vec3<i32>(mask) * (*state).step_dir;
    let normal = vec3<f32>(mask) * -vec3<f32>((*state).step_dir); 
    return normal;
}
```
Once a step has been performed, the 3D grid is interrogated at the position of the DDA cursor to see whether or not it's filled in.

# Results

Since there is no color information in the data I sent to the GPU, I had to find other ways to color the voxels, so I could make sure the code works.
Initially, I tried to use the box-normal function I used in the last post, without really thinking about how to do so. 
![some black and a few blue boxes]({{ site.baseurl }}/img/incorrect.jpg)
It didn't work, at all, but the black boxes did show me that the DDA algorithm was stepping through *some* data. Whether it was correct was still unclear.

Next, I tried coloring the voxels different shades of gray based on how far they were from the camera when hit.
![Voxels colored based on distance from camera]({{ site.baseurl }}/img/distance.jpg)
The distance was calculated based on the distance to the voxel, not the pixel drawn, so the voxels each had their own color, which I think looked pretty good for being grayscale. At this point it was clear that the data being used in the raytracing step was the data that I had provided in my code, but the lack of color made it hard to see
if it was in the correct order.

So my final method for this part of the project used the absolute value of the normal calculated in the `step_DDA` function above as the color. 
![Voxel normals]({{ site.baseurl }}/img/voxnormals.jpg)
With the normals as colors, I could easily confirm that the voxels were being put on the GPU in the correct order, and that the DDA algorithm was traversing them properly.

In the next post I will be trying to put more information about the voxels on the GPU, such as colors and materials, as well as structuring them more efficiently using chunks.