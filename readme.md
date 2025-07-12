# boba-cat

##### simple cat enjoying some boba on your desktop

<img src="docs/demo.gif" alt="">

## usage (`.deb` example)
* download `.deb` from [releases](https://github.com/lyova24/boba-cat/releases) or build it yourself
  ```shell
  # build using this
  npm run make
  
  # or this
  electron-forge make
  ```
* install
  ```shell
  # path/to/deb depends on the prev step
  # it is in /out/make/deb/... if you built app yourself
  dpkg -i path/to/deb
  ```
* run
  ```shell
  nohup /usr/bin/boba-cat/boba-cat >/dev/null 2>&1 &
  ```
