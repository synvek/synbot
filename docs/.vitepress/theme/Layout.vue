<template>
  <Layout>
    <template #nav-bar-content-after>
      <slot name="nav-bar-content-after" />
    </template>
    
    <template #nav-screen-content-before>
      <slot name="nav-screen-content-before" />
    </template>
    
    <template #nav-screen-content-after>
      <slot name="nav-screen-content-after" />
    </template>
    
    <template #sidebar-top>
      <slot name="sidebar-top" />
    </template>
    
    <template #sidebar-bottom>
      <slot name="sidebar-bottom" />
    </template>
    
    <template #aside-top>
      <slot name="aside-top" />
    </template>
    
    <template #aside-bottom>
      <slot name="aside-bottom" />
    </template>
    
    <template #aside-outline-before>
      <slot name="aside-outline-before" />
    </template>
    
    <template #aside-outline-after>
      <slot name="aside-outline-after" />
    </template>
    
    <template #aside-ads-before>
      <slot name="aside-ads-before" />
    </template>
    
    <template #aside-ads-after>
      <slot name="aside-ads-after" />
    </template>
    
    <template #doc-top>
      <slot name="doc-top" />
    </template>
    
    <template #doc-bottom>
      <slot name="doc-bottom" />
    </template>
    
    <template #doc-footer-before>
      <slot name="doc-footer-before" />
    </template>
    
    <template #doc-footer-after>
      <slot name="doc-footer-after" />
    </template>
    
    <template #home-hero-before>
      <slot name="home-hero-before" />
    </template>
    
    <template #home-hero-after>
      <slot name="home-hero-after" />
    </template>
    
    <template #home-features-before>
      <slot name="home-features-before" />
    </template>
    
    <template #home-features-after>
      <slot name="home-features-after" />
    </template>
  </Layout>
</template>

<script setup>
import DefaultTheme from 'vitepress/theme'

const { Layout } = DefaultTheme
</script>