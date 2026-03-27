interface ListItem {
  id: string;
  name: string;
  description?: string;
  itemCount: number;
  isSmart: boolean;
}

interface ListCardProps {
  list: ListItem;
}

export default function ListCard({ list }: ListCardProps) {
  return (
    <div
      data-testid="list-card"
      data-list-id={list.id}
      className={`list-card ${list.isSmart ? "list-card--smart" : ""}`}
    >
      {/* List card stub */}
      <h3 className="list-card-name">{list.name}</h3>
      {list.description && (
        <p className="list-card-description">{list.description}</p>
      )}
      <span className="list-card-count">{list.itemCount} items</span>
      {list.isSmart && <span className="list-card-smart-badge">Smart</span>}
    </div>
  );
}
