import "./style.css";
import { SearchBox } from "./search";
import { api } from "./api";
import { Galaxy } from "./galaxy";
import { RouteDialog } from "./route-dialog";
import * as wasm from "../rust-module/pkg";

async function main() {
  const galaxy = new Galaxy();
  await galaxy.init();
  let trie_bin: undefined | ArrayBuffer = undefined;

  // Create route dialog
  const routeDialog = new RouteDialog({
    onSuggest: (word: string) => {
      if (trie_bin) {
        return wasm.suggest_words(new Uint8Array(trie_bin), word, 10) as string[];
      }
      return [];
    }
  });

  const searchBox = new SearchBox({
    placeholder: "Enter target star..",
    onSearch: async (query: string) => {
      const target = await api.getStarCoords(query);
      galaxy.setTarget(target);
    },
    onSuggest: (word: string) => {
      if (trie_bin) {
        return wasm.suggest_words(new Uint8Array(trie_bin), word, 10) as string[];
      }
      return [];
    },
    onClickRoute: async (word: string) => {
      // Pre-fill the "to" field with the current search term
      routeDialog.setToValue(word);

      // Open dialog and wait for user configuration
      const routeConfig = await routeDialog.open();

      if (routeConfig) {
        console.log('Route configuration:', routeConfig);
        // TODO: Implement route generation with the configuration
        // You can use routeConfig.from, routeConfig.to, and routeConfig.alreadySupercharged
      }
    }
  });

  searchBox.mount(document.body);

  fetch("/data/search_trie.bin").then(res => res.arrayBuffer())
    .then(buffer => {
      trie_bin = buffer;
    })
}

main();
