use engine::find_yaml_file;
use engine::info::VPF;
use engine::matchers::{Favicon, MatcherType, Part};
use engine::request::HttpRaw;
use engine::template::Template;
use helper::cli::HelperConfig;
use helper::nmap::nmap;
use std::collections::BTreeMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::path::{Path};
use std::str::FromStr;

const UNKNOWN_VENDOR: &str = "00_unknown";
const BUILT_TAGS: [&str; 59] = [
  "misconfig",
  "fileupload",
  "intrusive",
  "php",
  "hackerone",
  "oob",
  "authenticated",
  "oss",
  "deserialization",
  "db",
  "c2",
  "panel",
  "detect",
  "default-login",
  "miscellaneous",
  "token-spray",
  "tech",
  "phishing",
  "cloud",
  "sql",
  "bypass",
  "exposure",
  "wp-theme",
  "dos",
  "info",
  "misc",
  "exposure",
  "config",
  "lfr",
  "auth",
  "login",
  "fuzz",
  "xxe",
  "packetstorm",
  "crlf",
  "auth-bypass",
  "injection",
  "ssti",
  "rfi",
  "redirect",
  "vulhub",
  "xmlswf",
  "plugin",
  "wp",
  "xss",
  "lfi",
  "edb",
  "cms",
  "sqli",
  "rce",
  "msf",
  "kev",
  "iot",
  "wordpress",
  "wp-plugin",
  "wpscan",
  "unauth",
  "ssrf",
  "oast",
];

fn remove_built_tags(tags: &[String]) -> Vec<String> {
  let mut tags = tags.to_owned();
  tags.retain(|x| {
    !(BUILT_TAGS.contains(&x.as_str())
      || x.starts_with("top-")
      || x.starts_with("cve")
      || x.starts_with("osint")
      || x.starts_with("cnvd"))
  });
  tags
}

fn sync_nuclei() {
  //同步nuclei
  let mut yaml_paths = Vec::new();
  for path in ["cnvd", "cves", "default-logins", "vulnerabilities"] {
    let y = format!("nuclei-templates/http/{}", path);
    yaml_paths.extend(find_yaml_file(&Path::new(&y).to_path_buf(), true));
  }
  let mut count = 0;
  let current_dir = env::current_dir().unwrap().join("plugins");
  for yaml_path in yaml_paths {
    if let Ok(f) = File::open(&yaml_path) {
      match serde_yaml::from_reader::<std::fs::File, Template>(f) {
        Ok(template) => {
          // 先挑选有cpe和厂商和产品的yaml。如果有分类复制文件后跳过
          if let Some(vpf) = template.info.get_vpf() {
            // 如果有框架的直接复制到框架文件夹，注：只针对wordpress
            let mut default_path = current_dir.join(&vpf.vendor).join(&vpf.product);
            if let Some(framework) = vpf.framework {
              if framework.as_str() == "wordpress" {
                let wp_path = current_dir.join("wordpress").join("wordpress");
                let _ = std::fs::remove_dir_all(default_path);
                // 如果剩下的文件夹为空，删除
                let _ = std::fs::remove_dir(&current_dir.join(&vpf.vendor));
                default_path = wp_path;
              }
            }
            std::fs::create_dir_all(&default_path).unwrap();
            let f_path =
              default_path.join(&yaml_path.file_name().unwrap().to_string_lossy().to_string());
            std::fs::copy(&yaml_path, &f_path).unwrap();
            count += 1;
            continue;
          }
          // 根据tags分类
          let tags = remove_built_tags(&template.info.tags);
          let mut has_vendor = false;
          let mut has_product = false;
          for tag in tags.iter() {
            if current_dir.join(tag).is_dir() {
              has_vendor = true;
              for sub_tag in tags.iter() {
                // 厂商和产品不一样
                if current_dir.join(tag).join(sub_tag).is_dir() && sub_tag != tag {
                  has_product = true;
                  let _ = std::fs::copy(
                    &yaml_path,
                    current_dir
                      .join(tag)
                      .join(sub_tag)
                      .join(yaml_path.file_name().unwrap().to_string_lossy().to_string()),
                  )
                  .unwrap();
                  break;
                }
              }
              if !has_product && tags.len() == 1 {
                has_product = true;
                let product = tags.first().unwrap();
                let _ = std::fs::create_dir_all(current_dir.join(tag).join(product));
                let _ = std::fs::copy(
                  &yaml_path,
                  current_dir
                    .join(tag)
                    .join(product)
                    .join(yaml_path.file_name().unwrap().to_string_lossy().to_string()),
                )
                .unwrap();
              }
              continue;
            }
          }
          if has_vendor {
            if !has_product {
              // 有厂商没有产品
              println!("{:?}", yaml_path);
              println!("{:?}", tags);
            }
          } else {
            // 什么都匹配不到
            // println!("{:?}", yaml_path);
            // println!("{:?}", tags);
          }
        }
        Err(err) => {
          println!("-----   {:?}   -----", yaml_path);
          println!("{:?}", err);
        }
      };
    }
  }
  println!("{}", count);
}

// 将有厂商和产品的指纹移动到已经分类好的文件夹
fn rename_fingerprint_yaml() {
  let current_plugin_dir = env::current_dir().unwrap().join("plugins");
  let current_fingerprint_dir = env::current_dir().unwrap().join("web-fingerprint");
  let unknown_yaml_paths = find_yaml_file(&current_fingerprint_dir.join(UNKNOWN_VENDOR), false);
  let all_plugins_vendor_name: Vec<String> = std::fs::read_dir(&current_plugin_dir)
    .unwrap()
    .map(|p| p.unwrap().file_name().to_string_lossy().to_string())
    .collect();
  for unknown_yaml_path in unknown_yaml_paths {
    if let Ok(f) = File::open(&unknown_yaml_path) {
      match serde_yaml::from_reader::<std::fs::File, Template>(f) {
        Ok(template) => {
          let vpf = template.info.get_vpf();
          if all_plugins_vendor_name.contains(&template.id) {
            let same = current_plugin_dir.join(&template.id);
            if same.is_dir() {
              let finger = same.join(format!("{}.yaml", &template.id));
              println!(
                "rename: {} to {}",
                unknown_yaml_path.to_string_lossy(),
                finger.to_string_lossy()
              );
              std::fs::rename(&unknown_yaml_path, finger).unwrap();
              continue;
            }
          }
          if let Some((v, p)) = template.id.split_once('-') {
            if all_plugins_vendor_name.contains(&v.to_string()) {
              let same = current_plugin_dir.join(v);
              let path = same.join(p);
              if path.is_dir() {
                let finger = same.join(format!("{}.yaml", p));
                println!(
                  "rename: {} to {}",
                  unknown_yaml_path.to_string_lossy(),
                  finger.to_string_lossy()
                );
                std::fs::rename(&unknown_yaml_path, finger).unwrap();
                continue;
              }
            }
          };
          if let Some(vpf) = vpf {
            if vpf.vendor == UNKNOWN_VENDOR {
              continue;
            }
            let p = current_plugin_dir.join(&vpf.vendor).join(&vpf.product);
            if p.exists() && p.is_dir() {
              std::fs::create_dir_all(current_fingerprint_dir.join(&vpf.vendor)).unwrap();
              let finger = current_fingerprint_dir
                .join(&vpf.vendor)
                .join(format!("{}.yaml", vpf.product));
              println!(
                "rename: {} to {}",
                unknown_yaml_path.to_string_lossy(),
                finger.to_string_lossy()
              );
              std::fs::rename(&unknown_yaml_path, finger).unwrap();
              continue;
            }
          }
        }
        Err(err) => {
          println!("{}", err)
        }
      }
    }
  }
  for name in all_plugins_vendor_name.iter() {
    for yaml_path in find_yaml_file(&current_plugin_dir.join(name), false) {
      let finger_path = current_fingerprint_dir.join(name);
      std::fs::create_dir_all(&finger_path).unwrap();
      std::fs::rename(&yaml_path, finger_path.join(yaml_path.file_name().unwrap())).unwrap();
    }
  }
}

fn update_template(template: &mut Template) {
  for http in template.requests.http.iter_mut() {
    for matchers in http.operators.matchers.iter_mut() {
      // 如果是关键词匹配，添加转小写和忽略大小写
      if let MatcherType::Word(mut w) = matchers.matcher_type.clone() {
        let new: Vec<String> = w.words.iter().map(|x| x.to_ascii_lowercase()).collect();
        w.words.clone_from(&new);
        matchers.matcher_type = MatcherType::Word(w);
        if let Part::Name(name) = &matchers.part {
          if name == "favicon" {
            matchers.part = Part::Body;
            matchers.matcher_type = MatcherType::Favicon(Favicon { hash: new });
          }
        }
        matchers.case_insensitive = true;
      }
      if let MatcherType::Favicon(mut h) = matchers.matcher_type.clone() {
        let new: Vec<String> = h.hash.iter().map(|x| x.to_ascii_lowercase()).collect();
        h.hash = new;
        matchers.matcher_type = MatcherType::Favicon(h);
        matchers.case_insensitive = false;
      }
    }
    // 路径请求方式转大写
    if let HttpRaw::Path(mut h) = http.http_raw.clone() {
      h.method =
        engine::slinger::http::Method::from_str(&h.method.as_str().to_uppercase()).unwrap();
      http.http_raw = HttpRaw::Path(h);
    }
  }
}

fn update_info(template: &mut Template, fingerprint_yaml_path: &Path) {
  let vpf = if let Some(pvf) = template.info.get_vpf() {
    pvf
  } else if let Some(parent) = fingerprint_yaml_path.parent() {
    let product = fingerprint_yaml_path
      .file_name()
      .unwrap_or_default()
      .to_string_lossy()
      .to_string()
      .trim_end_matches(".yaml")
      .to_string();
    let vendor = parent
      .file_name()
      .unwrap_or_default()
      .to_string_lossy()
      .to_string();
    let verified = vendor == UNKNOWN_VENDOR;
    VPF {
      vendor,
      product,
      framework: None,
      verified,
    }
  } else {
    VPF {
      vendor: UNKNOWN_VENDOR.to_string(),
      product: UNKNOWN_VENDOR.to_string(),
      framework: None,
      verified: false,
    }
  };
  let new_vpf = BTreeMap::from_iter([
    (
      "verified".to_string(),
      engine::serde_format::Value::Bool(vpf.vendor.as_str() != UNKNOWN_VENDOR),
    ),
    (
      "vendor".to_string(),
      engine::serde_format::Value::String(vpf.vendor),
    ),
    (
      "product".to_string(),
      engine::serde_format::Value::String(vpf.product),
    ),
  ]);
  for (k, v) in new_vpf {
    template.info.metadata.insert(k, v);
  }
}

fn format() {
  let current_fingerprint_dir = env::current_dir().unwrap().join("web-fingerprint");
  let all_fingerprint_path = find_yaml_file(&current_fingerprint_dir, true);
  for fingerprint_yaml_path in all_fingerprint_path {
    let f = File::open(&fingerprint_yaml_path).unwrap();
    let mut new_template = None;
    if let Ok(mut template) = serde_yaml::from_reader::<std::fs::File, Template>(f) {
      update_template(&mut template);
      update_info(&mut template, &fingerprint_yaml_path);
      new_template = Some(template);
    }
    if let Some(t) = new_template {
      let f = OpenOptions::new()
        .write(true)
        .create(true)
        .append(false)
        .truncate(true)
        .open(&fingerprint_yaml_path)
        .unwrap();
      serde_yaml::to_writer(f, &t).unwrap();
    }
  }
}

fn convert_json(dir: &str, filename: &str) {
  let current_fingerprint_dir = env::current_dir().unwrap().join(dir);
  let yaml_paths = find_yaml_file(&current_fingerprint_dir, true);
  let mut templates = Vec::new();
  for yaml_path in yaml_paths {
    if let Ok(f) = File::open(&yaml_path) {
      match serde_yaml::from_reader::<std::fs::File, Template>(f) {
        Ok(template) => {
          templates.push(template);
        }
        Err(err) => {
          println!("{}", err)
        }
      }
    }
  }
  let f = OpenOptions::new()
    .write(true)
    .create(true)
    .append(false)
    .truncate(true)
    .open(filename)
    .unwrap();
  serde_json::to_writer(f, &templates).unwrap();
}

fn main() {
  let config = HelperConfig::default();
  if config.convert {
    convert_json("web-fingerprint", "web_fingerprint_v4.json");
    convert_json("service-fingerprint", "service_fingerprint_v4.json");
  }
  if config.sync {
    sync_nuclei();
  }
  if config.format {
    format();
    rename_fingerprint_yaml();
  }
  if config.service {
    nmap();
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn py() {}
}