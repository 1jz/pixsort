# pixsort

Image/Video pixel sorter made in rust inspired by [Kim Asendorf](https://github.com/kimasendorf/ASDFPixelSort)

### Dependencies

FFmpeg - tested with v5.1.2

## Usage:
```
cargo install --path .
pixsort --help
```

### Arguments

| Argument | Flag | Description |
|----------|------|-------------|
input|-i|input file to sort
rate|-r|framerate for videos
threads|-t|numbers of threads to use for sorting
white-threshold|-w|luminance threshold for whiteness
black-threshold|-b|luminance threshold for blackness

# Examples
`pixsort -i 'input.mp4' -r 24 -b 0 -w 255 out.mp4`
![paprika](./examples/full_sort.gif)

`pixsort -i 'input.mp4' -r 24 out.mp4`
![paprika](./examples/threshold.gif)