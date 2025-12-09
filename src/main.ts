import './style.css'
import { SearchBox } from './search';
import { api } from './api';
import { Galaxy } from './galaxy';

async function main() {

  const galaxy = new Galaxy()
  await galaxy.init()

  const searchBox = new SearchBox({
    placeholder: "Search stars..",
    onSearch: async (query: string) => {
      const target = await api.getStarCoords(query)
      galaxy.setTarget(target)
    }
  })

  searchBox.mount(document.body)

}

main()
