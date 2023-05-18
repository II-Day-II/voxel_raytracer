---
layout: page
title: Home
tags: [GPU Voxel Raytracing, DH2323 Project]
author: David Kaméus
---

# Welcome!

This is the blog for my computer graphics project in the course DH2323 at KTH Royal Institute of Technology.
Start reading from the beginning by scrolling down to the Project Outline post title!

<ul class="posts">
    {% for post in site.posts %}
        <li><span>{{ post.date | date_to_string }}</span> &raquo; <a href="{{ site.baseurl }}{{ post.url }}">{{ post.title }}</a></li>
    {% endfor %}
</ul>
